#!/usr/bin/env bash
#
# stress-test.sh — exhaustive, self-contained stress test for the `lattice` CLI.
#
# It builds the binary, spins up a throwaway environment containing a
# production-shaped monorepo spanning several languages, plus focused sub-repos,
# exercises every command, flag, and code path the tool exposes, asserts the
# observable behavior of each, and then tears the environment down.
#
# It is deterministic and hermetic: it needs no network and no language
# toolchains beyond a POSIX `sh`. Driver auto-detection for ~16 ecosystems is
# verified through `--dry-run` (which resolves commands without running them);
# real execution, toolchain provisioning, caching, and PATH injection are
# driven through portable shell scripts and a fake, locally-installed toolchain.
#
# Exit status: 0 iff every single assertion passed; non-zero otherwise.
#
# Env knobs:
#   LATTICE_STRESS_RELEASE=1   build/test the release binary instead of debug
#   KEEP_ENV=1                 do not delete the temp environment on exit
#
set -u

# ---------------------------------------------------------------------------
# Locate the repo, build the binary.
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

BOLD=""; DIM=""; RED=""; GRN=""; YEL=""; RST=""
if [ -t 1 ]; then
  BOLD="$(printf '\033[1m')"; DIM="$(printf '\033[2m')"; RED="$(printf '\033[31m')"
  GRN="$(printf '\033[32m')"; YEL="$(printf '\033[33m')"; RST="$(printf '\033[0m')"
fi

say()  { printf '%s\n' "$*"; }
sect() { printf '\n%s== %s ==%s\n' "$BOLD" "$*" "$RST"; }

PROFILE_FLAG=""
PROFILE_DIR="debug"
if [ "${LATTICE_STRESS_RELEASE:-0}" = "1" ]; then
  PROFILE_FLAG="--release"
  PROFILE_DIR="release"
fi

sect "Building lattice ($PROFILE_DIR)"
if ! cargo build $PROFILE_FLAG --bin lattice; then
  say "${RED}cargo build failed — cannot stress-test.${RST}"
  exit 1
fi
BIN="$REPO_ROOT/target/$PROFILE_DIR/lattice"
if [ ! -x "$BIN" ]; then
  say "${RED}built binary not found at $BIN${RST}"
  exit 1
fi

VERSION="$("$BIN" version --json 2>/dev/null | sed -n 's/.*"version":"\([^"]*\)".*/\1/p')"
if [ -z "$VERSION" ]; then
  say "${RED}\`version --json\` produced no version field${RST}"
  exit 1
fi
say "binary: $BIN"
say "version: $VERSION"

# ---------------------------------------------------------------------------
# Temp environment + cleanup.
# ---------------------------------------------------------------------------
ENVROOT="$(mktemp -d "${TMPDIR:-/tmp}/lattice-stress.XXXXXX")"
BG_PID=""

cleanup() {
  [ -n "$BG_PID" ] && kill -9 "$BG_PID" 2>/dev/null
  if [ "${KEEP_ENV:-0}" = "1" ]; then
    say "\n${YEL}KEEP_ENV=1 — leaving environment at:${RST} $ENVROOT"
  else
    rm -rf "$ENVROOT"
  fi
}
trap cleanup EXIT INT TERM

say "environment: $ENVROOT"

# ---------------------------------------------------------------------------
# Assertion harness.
# ---------------------------------------------------------------------------
PASS=0
FAIL=0
FAILED_NAMES=()
RC=0
OUTPUT=""

pass() { PASS=$((PASS + 1)); printf '  %sok%s   %s\n' "$GRN" "$RST" "$1"; }
fail() {
  FAIL=$((FAIL + 1)); FAILED_NAMES+=("$1")
  printf '  %sFAIL%s %s\n' "$RED" "$RST" "$1"
  [ -n "${2:-}" ] && printf '       %s%s%s\n' "$DIM" "$2" "$RST"
}

# Snippet of captured output, one line, for failure diagnostics.
snip() { printf '%s' "$OUTPUT" | tr '\n' '|' | cut -c1-400; }

# Run the binary in a working dir. Usage: lat <dir> <args...>
lat() {
  local dir="$1"; shift
  OUTPUT="$(cd "$dir" && "$BIN" "$@" 2>&1)"; RC=$?
}
# Same, with extra environment. Usage: late "VAR=val ..." <dir> <args...>
late() {
  local envs="$1"; local dir="$2"; shift 2
  OUTPUT="$(cd "$dir" && env $envs "$BIN" "$@" 2>&1)"; RC=$?
}

have()  { printf '%s\n' "$OUTPUT" | grep -qF -- "$1"; }
haveE() { printf '%s\n' "$OUTPUT" | grep -qE -- "$1"; }

t_ok()    { if [ "$RC" -eq 0 ]; then pass "$1"; else fail "$1" "exit=$RC | $(snip)"; fi; }
t_bad()   { if [ "$RC" -ne 0 ]; then pass "$1"; else fail "$1" "expected non-zero exit | $(snip)"; fi; }
t_has()   { if have  "$2"; then pass "$1"; else fail "$1" "missing [$2] | $(snip)"; fi; }
t_hasE()  { if haveE "$2"; then pass "$1"; else fail "$1" "missing /$2/ | $(snip)"; fi; }
t_hasnt() { if have  "$2"; then fail "$1" "unexpected [$2] | $(snip)"; else pass "$1"; fi; }
t_file()  { if [ -e "$1" ]; then pass "$2"; else fail "$2" "missing file $1"; fi; }
t_nofile(){ if [ -e "$1" ]; then fail "$2" "unexpected file $1"; else pass "$2"; fi; }
t_grepfile() { if grep -qF -- "$2" "$1" 2>/dev/null; then pass "$3"; else fail "$3" "[$2] not in $1"; fi; }

w() { mkdir -p "$(dirname "$1")"; printf '%s' "$2" > "$1"; }

# Anything TTY-only (label colors, the interactive display) is invisible to
# `lat`, which captures through a pipe. `ptylat` runs the binary under a
# pseudo-terminal instead, using whichever `script` the platform ships. BSD
# `script` does not propagate the child's exit status, so pty runs assert on
# captured output only, never on RC.
PTY_OK=1
command -v script >/dev/null 2>&1 || PTY_OK=0
PTY_FLAVOR="bsd"
script --version 2>/dev/null | grep -q util-linux && PTY_FLAVOR="util-linux"

# Usage: ptylat "VAR=val ..." <dir> <args...>
ptylat() {
  local envs="$1"; local dir="$2"; shift 2
  if [ "$PTY_FLAVOR" = "util-linux" ]; then
    OUTPUT="$(cd "$dir" && env $envs script -qe -c "$(printf '%q ' "$BIN" "$@")" /dev/null 2>&1)"
  else
    OUTPUT="$(cd "$dir" && env $envs script -q /dev/null "$BIN" "$@" 2>&1)"
  fi
}

# The prefix of a 24-bit foreground color escape — what a painted label starts with.
TRUECOLOR="$(printf '\033[38;2;')"

# =========================================================================
# 1. Top-level surfaces: bare, help, version, completions.
# =========================================================================
sect "Top-level surfaces"

lat "$ENVROOT" ; t_ok "bare \`lattice\` exits 0"
t_has "bare \`lattice\` points at help" "lattice --help"

lat "$ENVROOT" --help ; t_ok "\`--help\` exits 0"
t_has "help lists run"         "Run one or more tasks"
t_has "help lists setup"       "Provision"
t_has "help lists init"        "Scaffold"
t_has "help lists prune"       "Evict cache"
t_has "help lists upgrade"     "another version of Lattice"
t_has "help lists completions" "completion"
t_has "help lists version"     "version information"
t_has "help lists --theme"            "--theme"
t_has "help lists --release-base-url" "--release-base-url"

lat "$ENVROOT" --version ; t_ok "\`--version\` exits 0"
t_has "\`--version\` prints version" "$VERSION"

lat "$ENVROOT" version ; t_ok "\`version\` exits 0"
t_has "\`version\` splash mentions monorepos" "monorepos"
t_has "\`version\` splash shows version"      "$VERSION"

# --theme picks the splash shade; both shades render the same mark, and a shade
# that does not exist is refused rather than quietly ignored.
lat "$ENVROOT" version --theme light ; t_ok "\`version --theme light\` exits 0"
t_has "the light splash still shows the version" "$VERSION"
lat "$ENVROOT" --theme dark version ; t_ok "\`--theme\` works ahead of the subcommand"
t_has "the dark splash still shows the version"  "$VERSION"
lat "$ENVROOT" version --theme teal ; t_bad "\`--theme\` rejects a shade it does not have"
t_has "the bad-theme message lists the shades" "light"
# The variable the flag replaced still works when no flag is given.
late "LATTICE_THEME=light" "$ENVROOT" version ; t_ok "LATTICE_THEME still works"
t_has "the env-themed splash still shows the version" "$VERSION"

lat "$ENVROOT" version --json ; t_ok "\`version --json\` exits 0"
t_has "version json has version field" "\"version\":\"$VERSION\""
t_has "version json has target field"  "\"target\""
# A bare arch ("aarch64") is not a target triple; the installer needs the triple.
t_hasE "version json target is a triple" '"target":"[a-z0-9_]+-[a-z0-9_-]+"'

lat "$ENVROOT" run --help ; t_ok "\`run --help\` exits 0"
t_has "run help documents --filter"   "--filter"
t_has "run help documents --continue" "--continue"

for sh in bash zsh fish powershell elvish; do
  lat "$ENVROOT" completions "$sh"
  t_ok "completions $sh exits 0"
  if [ -n "$OUTPUT" ]; then pass "completions $sh non-empty"; else fail "completions $sh non-empty" "empty output"; fi
done
lat "$ENVROOT" completions notarealshell ; t_bad "completions rejects unknown shell"

# =========================================================================
# 2. init: skeleton, artifacts, gitignore, force, guard.
# =========================================================================
sect "init"

INITDIR="$ENVROOT/initdir"
mkdir -p "$INITDIR"
w "$INITDIR/.gitignore" "node_modules/
"
lat "$INITDIR" init --yes ; t_ok "init --yes exits 0"
t_has "init reports lattice.json"  "wrote lattice.json"
t_has "init reports schema.json"   "wrote .lattice/schema.json"
t_has "init reports gitignore"     "updated .gitignore"
t_file "$INITDIR/lattice.json"          "init created lattice.json"
t_file "$INITDIR/.lattice/schema.json"  "init created schema.json"
t_grepfile "$INITDIR/lattice.json" "\"build\"" "skeleton has a build task"
t_grepfile "$INITDIR/lattice.json" "$VERSION"  "skeleton pins current version"
t_grepfile "$INITDIR/.gitignore" ".lattice/cache/"      "gitignore has cache/"
t_grepfile "$INITDIR/.gitignore" ".lattice/toolchains/" "gitignore has toolchains/"
t_grepfile "$INITDIR/.gitignore" ".lattice/bin/"        "gitignore has bin/"
t_grepfile "$INITDIR/.gitignore" "node_modules/"        "gitignore preserves existing lines"
if python3 -c "import json,sys; json.load(open('$INITDIR/.lattice/schema.json'))" 2>/dev/null; then
  pass "schema.json is valid JSON (python3 check)"
elif "$BIN" run x >/dev/null 2>&1; then :; else
  pass "schema.json JSON check skipped (no python3)"
fi

lat "$INITDIR" init --yes ; t_bad "init refuses to clobber existing config"
t_has "init clobber message" "already exists"
lat "$INITDIR" init --yes --force ; t_ok "init --force overwrites"

# Self-heal: a missing schema (wiped cache dir, uncommitted clone) is rewritten
# by any command that loads the config, so editors always resolve `$schema`.
rm -f "$INITDIR/.lattice/schema.json"
lat "$INITDIR" run build --dry-run >/dev/null 2>&1 || true
t_file "$INITDIR/.lattice/schema.json" "run rewrites a missing schema (self-heal)"

# =========================================================================
# 3. The production monorepo.
# =========================================================================
sect "Generating the production monorepo"

PROD="$ENVROOT/prod"
ORDER="$PROD/.order"
mkdir -p "$PROD"/libs/core/src "$PROD"/services/api "$PROD"/apps/web "$PROD"/services/worker "$PROD"/docs
w "$PROD/libs/core/src/lib.src" "core source v1
"

# lattice.json is written with a quoted heredoc (no shell expansion), then the
# __ORDER__ placeholder is replaced with the absolute order-log path via sed.
sed "s#__ORDER__#$ORDER#g" > "$PROD/lattice.json" <<'JSON'
{
  "latticeVersion": "LATTICE_VERSION",
  "engines": {
    "faketool": {
      "version": ">=1.0.0",
      "versionCmd": "faketool",
      "bin": "bin",
      "installCmd": "mkdir -p '$LATTICE_TOOLCHAIN_DIR/bin' && echo '#!/bin/sh' > '$LATTICE_TOOLCHAIN_DIR/bin/faketool' && echo 'echo faketool 1.4.2' >> '$LATTICE_TOOLCHAIN_DIR/bin/faketool' && chmod +x '$LATTICE_TOOLCHAIN_DIR/bin/faketool'"
    },
    "hosttool": { "version": ">=1.0.0", "versionCmd": "echo 9.9.9" },
    "anytool": {}
  },
  "workspaces": [
    { "name": "core", "path": "libs/core", "auto": false, "scripts": {
      "build": "mkdir -p dist && echo core-lib > dist/lib.txt && echo core >> '__ORDER__'",
      "test": "test -f dist/lib.txt && echo core-test-ok",
      "lint": "echo core-lint-ok",
      "clean": "rm -rf dist && echo core-clean-ok",
      "envtask": "echo envval $STRESS_VAR",
      "nocache": "echo nocache-ran"
    } },
    { "name": "api", "path": "services/api", "auto": false, "dependsOn": ["core"], "scripts": {
      "build": "mkdir -p dist && faketool > dist/api.txt && echo api >> '__ORDER__'",
      "test": "echo api-test-ok",
      "lint": "echo api-lint-ok",
      "clean": "rm -rf dist && echo api-clean-ok"
    } },
    { "name": "web", "path": "apps/web", "auto": false, "dependsOn": ["core"], "scripts": {
      "build": "mkdir -p dist && echo '<html></html>' > dist/index.html && echo web >> '__ORDER__'",
      "test": "echo web-test-ok",
      "lint": "echo web-lint-ok",
      "clean": "rm -rf dist && echo web-clean-ok"
    } },
    { "name": "worker", "path": "services/worker", "auto": false, "dependsOn": ["api"], "scripts": {
      "build": "mkdir -p dist && echo worker-bin > dist/worker && echo worker >> '__ORDER__'",
      "test": "echo worker-test-ok",
      "lint": "echo worker-lint-ok",
      "clean": "rm -rf dist && echo worker-clean-ok"
    } },
    { "name": "docs", "path": "docs", "auto": false, "scripts": {
      "build": "mkdir -p dist && echo docs > dist/docs.txt",
      "test": "echo docs-test-ok",
      "lint": "echo docs-lint-ok",
      "clean": "rm -rf dist && echo docs-clean-ok",
      "dev": "echo READY_DEV && sleep 30"
    } }
  ],
  "tasks": {
    "build":   { "dependsOn": ["^build"], "inputs": ["src/**"], "outputs": ["dist/**"], "ignore": ["**/*.md"] },
    "test":    { "dependsOn": ["build"], "inputs": ["src/**", "tests/**"] },
    "lint":    { "inputs": ["src/**"] },
    "clean":   {},
    "envtask": { "env": ["STRESS_VAR"] },
    "nocache": { "cache": false },
    "dev":     { "persistent": true, "dependsOn": ["^build"] }
  },
  "settings": { "maxCacheSize": "1GB", "loquacious": false, "versionCheck": true }
}
JSON
# Patch the version placeholder (kept out of the sed-substituted heredoc body).
sed "s#LATTICE_VERSION#$VERSION#" "$PROD/lattice.json" > "$PROD/lattice.json.tmp" && mv "$PROD/lattice.json.tmp" "$PROD/lattice.json"

# Sanity: config must load.
lat "$PROD" run definitely-not-a-task ; t_bad "prod config loads (unknown task still parses config)"
t_has "unknown task names the task"     "definitely-not-a-task"
t_has "unknown task lists available"    "available tasks:"

# =========================================================================
# 4. setup / toolchain gradient (host / validate / provision).
# =========================================================================
sect "setup & toolchains"

lat "$PROD" setup ; t_ok "setup exits 0"
t_has "setup completes" "setup complete"
FAKEBIN="$(find "$PROD/.lattice/toolchains/faketool" -name faketool -type f 2>/dev/null | head -1)"
if [ -n "$FAKEBIN" ] && [ -x "$FAKEBIN" ]; then pass "provisioned faketool binary exists"; else fail "provisioned faketool binary exists" "not found under .lattice/toolchains"; fi
t_file "$PROD/.lattice/toolchains/faketool" "toolchains dir created under .lattice"

lat "$PROD" setup ; t_ok "setup re-run exits 0 (pin reused)"
lat "$PROD" setup --force ; t_ok "setup --force exits 0"
lat "$PROD" setup core ; t_ok "setup <workspace> scopes cleanly"

# =========================================================================
# 5. run: ordering, dependencies, dry-run, filter.
# =========================================================================
sect "run — ordering, dependencies, dry-run, filter"

rm -f "$ORDER"
lat "$PROD" run build --no-cache ; t_ok "run build (full, --no-cache) exits 0"
t_has "run summary printed" "lattice:"
if [ -f "$ORDER" ]; then
  ci="$(grep -n '^core$'   "$ORDER" | head -1 | cut -d: -f1)"
  ai="$(grep -n '^api$'    "$ORDER" | head -1 | cut -d: -f1)"
  wi="$(grep -n '^web$'    "$ORDER" | head -1 | cut -d: -f1)"
  ki="$(grep -n '^worker$' "$ORDER" | head -1 | cut -d: -f1)"
  if [ -n "$ci$ai$wi$ki" ] && [ "$ci" -lt "$ai" ] && [ "$ci" -lt "$wi" ] && [ "$ai" -lt "$ki" ]; then
    pass "cross-workspace order: core→api→worker and core→web"
  else
    fail "cross-workspace order: core→api→worker and core→web" "order=$(tr '\n' ',' < "$ORDER")"
  fi
else
  fail "cross-workspace order: core→api→worker and core→web" "no order log written"
fi

# --dry-run must not execute (delete an output, dry-run, confirm not recreated).
rm -rf "$PROD/apps/web/dist"
lat "$PROD" run build --dry-run ; t_ok "run build --dry-run exits 0"
t_has "dry-run banner"        "dry run"
t_hasE "dry-run lists a node" "web:build"
t_nofile "$PROD/apps/web/dist" "dry-run performed no work"

lat "$PROD" run build --filter core ; t_ok "run --filter core exits 0"
t_hasnt "filter excludes other workspaces" "web:build"
lat "$PROD" run build --filter nonexistent ; t_ok "run --filter no-match exits 0"
t_has "no-match filter is a clean no-op" "no workspaces matched"

# =========================================================================
# 6. Caching: store, hit, invalidate, cache:false, env-keyed, --force.
# =========================================================================
sect "caching"

# Prime + hit.
lat "$PROD" run build --filter core ; t_ok "run build core (prime cache) exits 0"
rm -f "$PROD/libs/core/dist/lib.txt"
lat "$PROD" run build --filter core ; t_ok "run build core (second) exits 0"
t_has "cache hit on unchanged inputs" "cache hit"
t_file "$PROD/libs/core/dist/lib.txt" "cache hit restored outputs"

# Invalidate via a tracked input file.
w "$PROD/libs/core/src/lib.src" "core source v2 CHANGED
"
lat "$PROD" run build --filter core ; t_ok "run build core after edit exits 0"
t_hasnt "edited input busts the cache" "cache hit"
t_has  "busted key re-runs the task"   "core:build"

# --no-cache / --force ignore the cache entirely.
lat "$PROD" run build --filter core --no-cache ; t_hasnt "--no-cache never hits cache" "cache hit"
lat "$PROD" run build --filter core --force    ; t_hasnt "--force never hits cache"    "cache hit"

# cache:false task is never cached.
lat "$PROD" run nocache --filter core ; t_ok "nocache run 1 exits 0"
lat "$PROD" run nocache --filter core ; t_ok "nocache run 2 exits 0"
t_hasnt "cache:false task is never a hit" "cache hit"
t_has   "cache:false task always runs"    "core:nocache: running"

# env-keyed cache: same value hits, different value misses.
late "STRESS_VAR=alpha" "$PROD" run envtask --filter core ; t_ok "envtask (alpha) prime exits 0"
late "STRESS_VAR=alpha" "$PROD" run envtask --filter core ; t_has "same env value → cache hit" "cache hit"
late "STRESS_VAR=beta"  "$PROD" run envtask --filter core ; t_hasnt "changed env value → cache miss" "cache hit"

# Full cache: a run where every scheduled task came back from cache is called
# out; a partial hit and a run that scheduled nothing are not.
lat "$PROD" run build ; t_ok "run build (whole repo, prime) exits 0"
lat "$PROD" run build ; t_ok "run build (whole repo, all cached) exits 0"
t_has "a fully-cached run is called out" "full cache"

# `docs` depends on nothing, so busting it leaves every other task a hit.
w "$PROD/docs/src/page.src" "docs page v1
"
lat "$PROD" run build ; t_ok "run build after leaf edit exits 0"
t_has   "the busted leaf re-ran"                "docs:build"
t_has   "its siblings still hit the cache"      "cache hit"
t_hasnt "a partial hit is not a full cache"     "full cache"
lat "$PROD" run build ; t_has "the leaf edit settles back to a full cache" "full cache"

lat "$PROD" run build --filter nonexistent ; t_hasnt "a run that scheduled nothing is not a full cache" "full cache"

# =========================================================================
# 7. run: PATH injection, concurrency, loquacious, other tasks.
# =========================================================================
sect "run — PATH injection, concurrency, verbosity, other tasks"

lat "$PROD" run build --filter api --no-cache ; t_ok "run build api exits 0"
t_grepfile "$PROD/services/api/dist/api.txt" "faketool 1.4.2" "provisioned tool resolved via injected PATH"

lat "$PROD" run build --no-cache --concurrency 1 ; t_ok "run --concurrency 1 exits 0"
lat "$PROD" run build --no-cache --concurrency 4 ; t_ok "run --concurrency 4 exits 0"

lat "$PROD" run build --filter core --no-cache -l ; t_ok "run -l (loquacious) exits 0"
t_has "loquacious emits hash trace" "hash"
t_hasnt "piped -l emits no ANSI escapes" "$TRUECOLOR"
lat "$PROD" run build --filter core --no-cache -v ; t_ok "hidden -v alias exits 0"

# Label colors: `-l` at a real terminal paints each `workspace:task` label its
# own color, and every task in the run gets a different one. Piped (asserted
# above) and under NO_COLOR, the same run emits nothing to strip.
if [ "$PTY_OK" = "1" ]; then
  ptylat "" "$PROD" run build --no-cache -l
  t_has "-l under a pty colors the workspace:task label" "$TRUECOLOR"
  HUES="$(printf '%s\n' "$OUTPUT" | grep -o "$TRUECOLOR"'[0-9;]*m' | sort -u | wc -l | tr -d ' ')"
  if [ "${HUES:-0}" -ge 2 ]; then
    pass "each task's label gets a distinct color ($HUES in the run)"
  else
    fail "each task's label gets a distinct color" "only $HUES distinct | $(snip)"
  fi
  ptylat "NO_COLOR=1" "$PROD" run build --no-cache -l
  t_hasnt "NO_COLOR suppresses label color under a pty" "$TRUECOLOR"
else
  say "  ${YEL}skip${RST} label-color assertions (no \`script\` to allocate a pty)"
fi

lat "$PROD" run build --filter core --no-cache --no-version-check ; t_ok "--no-version-check accepted"

# -l so the CI reporter streams task output (echoed markers) we assert on.
lat "$PROD" run test  --no-cache -l ; t_ok "run test (full) exits 0"
t_has "test ran downstream of build" "core-test-ok"
lat "$PROD" run lint  -l ; t_ok "run lint exits 0"
t_has "lint ran"  "core-lint-ok"
lat "$PROD" run clean -l ; t_ok "run clean exits 0"
t_has "clean ran" "core-clean-ok"

# Stacked commands: one invocation, one combined graph. lint + test + build run
# together; test's build dependency runs once, ahead of test.
lat "$PROD" run lint test build --no-cache -l ; t_ok "run lint test build (stacked) exits 0"
t_has "stacked run ran lint"  "core-lint-ok"
t_has "stacked run ran test"  "core-test-ok"
t_has "stacked run built"     "core:build"
lat "$PROD" run lint test build --dry-run ; t_ok "stacked --dry-run exits 0"
t_has "stacked dry-run banner lists all roots" "dry run · lint test build"
lat "$PROD" run build definitely-not-a-task ; t_bad "stacked run rejects an unknown task"
t_has "unknown stacked task names the offender" "definitely-not-a-task"

# --sequentially: each task's graph runs to completion before the next, in order.
lat "$PROD" run lint test -s --no-cache -l ; t_ok "run lint test --sequentially exits 0"
t_has "sequential run ran lint"  "core-lint-ok"
t_has "sequential run ran test"  "core-test-ok"
lat "$PROD" run lint build --sequentially --dry-run ; t_ok "sequential --dry-run exits 0"
t_has "sequential dry-run labels the lint phase"  "dry run · lint (phase)"
t_has "sequential dry-run labels the build phase" "dry run · build (phase)"

# =========================================================================
# 8. Persistent task (dev server): must not block; SIGINT tears down.
# =========================================================================
sect "persistent tasks"

DEVLOG="$ENVROOT/dev.log"
: > "$DEVLOG"
# No -l: a run that pulls in a persistent task auto-selects raw, line-by-line
# output so the dev server's streaming output stays visible.
( cd "$PROD" && exec "$BIN" run dev --filter docs ) > "$DEVLOG" 2>&1 &
BG_PID=$!
# Wait for the persistent child to announce readiness (or the process to die).
i=0
while [ $i -lt 150 ]; do
  grep -q "READY_DEV" "$DEVLOG" 2>/dev/null && break
  kill -0 "$BG_PID" 2>/dev/null || break
  sleep 0.1; i=$((i + 1))
done
if grep -q "READY_DEV" "$DEVLOG" 2>/dev/null; then pass "persistent task started and detached"; else fail "persistent task started and detached" "no READY_DEV in $i ticks"; fi
# Interrupt; the graph should have already drained the non-persistent work.
kill -INT "$BG_PID" 2>/dev/null
j=0
while kill -0 "$BG_PID" 2>/dev/null && [ $j -lt 60 ]; do sleep 0.1; j=$((j + 1)); done
if kill -0 "$BG_PID" 2>/dev/null; then
  kill -9 "$BG_PID" 2>/dev/null; wait "$BG_PID" 2>/dev/null
  fail "persistent run terminates on SIGINT" "still running after 6s — hung"
else
  wait "$BG_PID" 2>/dev/null
  pass "persistent run terminates on SIGINT"
fi
BG_PID=""
if grep -q "docs:build" "$DEVLOG" 2>/dev/null; then pass "persistent run drained its build prerequisite"; else fail "persistent run drained its build prerequisite" "$(tr '\n' '|' < "$DEVLOG" | cut -c1-300)"; fi

# =========================================================================
# 9. Keep-going vs fail-fast.
# =========================================================================
sect "failure handling"

FAILREPO="$ENVROOT/failrepo"
mkdir -p "$FAILREPO"/a "$FAILREPO"/b
cat > "$FAILREPO/lattice.json" <<'JSON'
{
  "workspaces": [
    { "name": "a", "path": "a", "auto": false, "scripts": { "build": "exit 3", "test": "echo a-test-ran" } },
    { "name": "b", "path": "b", "auto": false, "scripts": { "build": "echo b-build-ok", "test": "echo b-test-ran" } }
  ],
  "tasks": { "build": {}, "test": { "dependsOn": ["build"] } }
}
JSON
lat "$FAILREPO" run build ; t_bad "fail-fast: failing task yields non-zero exit"
t_has "fail-fast surfaces the failure" "FAILED"

# -l so skip notices and streamed output are visible for assertions.
lat "$FAILREPO" run test --continue -l ; t_bad "--continue still exits non-zero when a task failed"
t_has "--continue skips downstream of failure" "a:test: skipped"
t_has "--continue runs independent work"        "b-test-ran"

# =========================================================================
# 10. Driver auto-detection across ecosystems (via --dry-run).
# =========================================================================
sect "driver detection (never-prescribe) — all ecosystems"

DET="$ENVROOT/detect"
PKG='{ "scripts": { "build": "tsc" } }'
# npm also declares a `dev` script, so a persistent `dev` task resolves here,
# but must not be fabricated for the direct-invoke drivers (cargo/go/…).
PKG_DEV='{ "scripts": { "build": "tsc", "dev": "vite" } }'
mkdir -p "$DET"
w "$DET/pkgs/npm/package-lock.json" "";      w "$DET/pkgs/npm/package.json" "$PKG_DEV"
w "$DET/pkgs/pnpm/pnpm-lock.yaml" "";         w "$DET/pkgs/pnpm/package.json" "$PKG"
w "$DET/pkgs/yarn/yarn.lock" "";              w "$DET/pkgs/yarn/package.json" "$PKG"
w "$DET/pkgs/bun/bun.lockb" "";               w "$DET/pkgs/bun/package.json" "$PKG"
w "$DET/pkgs/deno/deno.json" '{ "tasks": { "build": "echo x" } }'
w "$DET/pkgs/cargo/Cargo.lock" "";            w "$DET/pkgs/cargo/Cargo.toml" "[package]
name=\"c\""
w "$DET/pkgs/go/go.sum" "";                   w "$DET/pkgs/go/go.mod" "module c
go 1.21"
w "$DET/pkgs/uv/uv.lock" "";                  w "$DET/pkgs/uv/pyproject.toml" "[project]"
w "$DET/pkgs/poetry/poetry.lock" "";          w "$DET/pkgs/poetry/pyproject.toml" "[project]"
w "$DET/pkgs/bundler/Gemfile.lock" "";        w "$DET/pkgs/bundler/Gemfile" "source 'x'"
w "$DET/pkgs/rake/Rakefile" "task :build"
w "$DET/pkgs/gradle/gradlew" "#!/bin/sh";     w "$DET/pkgs/gradle/build.gradle" ""
w "$DET/pkgs/maven/mvnw" "#!/bin/sh";         w "$DET/pkgs/maven/pom.xml" "<project/>"
w "$DET/pkgs/dotnet/global.json" "{}"
w "$DET/pkgs/pdm/pdm.lock" "";                w "$DET/pkgs/pdm/pyproject.toml" "[project]"
w "$DET/pkgs/pipenv/Pipfile.lock" "";         w "$DET/pkgs/pipenv/Pipfile" ""
w "$DET/pkgs/pip/requirements.txt" "flask"
w "$DET/pkgs/nuget/packages.config" "<packages/>"
w "$DET/pkgs/pod/Podfile.lock" "";            w "$DET/pkgs/pod/Podfile" "platform :ios"
w "$DET/pkgs/swift/Package.resolved" "{}";    w "$DET/pkgs/swift/Package.swift" ""
w "$DET/pkgs/composer/composer.lock" "";      w "$DET/pkgs/composer/composer.json" "{}"
w "$DET/pkgs/mix/mix.lock" "";                w "$DET/pkgs/mix/mix.exs" ""
w "$DET/pkgs/dart/pubspec.lock" "";           w "$DET/pkgs/dart/pubspec.yaml" "name: d"
w "$DET/pkgs/stack/stack.yaml.lock" "";       w "$DET/pkgs/stack/stack.yaml" ""
w "$DET/pkgs/cabal/cabal.project.freeze" ""
w "$DET/pkgs/shrinkwrap/npm-shrinkwrap.json" "{}"; w "$DET/pkgs/shrinkwrap/package.json" "$PKG"
# An SDK-style .NET project may carry a nuget lockfile and still be dotnet-driven.
w "$DET/pkgs/dotnet-locked/global.json" "{}"; w "$DET/pkgs/dotnet-locked/packages.lock.json" "{}"
# kotlin is a runtime: it composes under gradle rather than driving.
w "$DET/pkgs/kotlin/.tool-versions" "kotlin 2.0.0"; w "$DET/pkgs/kotlin/gradlew" "#!/bin/sh"
w "$DET/pkgs/override/pnpm-lock.yaml" "";     w "$DET/pkgs/override/package.json" "$PKG"
w "$DET/pkgs/composition/.nvmrc" "20";        w "$DET/pkgs/composition/pnpm-lock.yaml" "";  w "$DET/pkgs/composition/package.json" "$PKG"
# bun is a runtime *and* a package manager; it outranks a bare node runtime.
w "$DET/pkgs/dual-role/.nvmrc" "20";          w "$DET/pkgs/dual-role/bun.lockb" "";         w "$DET/pkgs/dual-role/package.json" "$PKG"
cat > "$DET/lattice.json" <<'JSON'
{
  "workspaces": [
    { "name": "npm", "path": "pkgs/npm" },
    { "name": "pnpm", "path": "pkgs/pnpm" },
    { "name": "yarn", "path": "pkgs/yarn" },
    { "name": "bun", "path": "pkgs/bun" },
    { "name": "deno", "path": "pkgs/deno" },
    { "name": "cargo", "path": "pkgs/cargo" },
    { "name": "go", "path": "pkgs/go" },
    { "name": "uv", "path": "pkgs/uv" },
    { "name": "poetry", "path": "pkgs/poetry" },
    { "name": "bundler", "path": "pkgs/bundler" },
    { "name": "rake", "path": "pkgs/rake" },
    { "name": "gradle", "path": "pkgs/gradle" },
    { "name": "maven", "path": "pkgs/maven" },
    { "name": "dotnet", "path": "pkgs/dotnet" },
    { "name": "pdm", "path": "pkgs/pdm" },
    { "name": "pipenv", "path": "pkgs/pipenv" },
    { "name": "pip", "path": "pkgs/pip", "engines": { "pip": ">=0.0.0" } },
    { "name": "nuget", "path": "pkgs/nuget" },
    { "name": "pod", "path": "pkgs/pod" },
    { "name": "swift", "path": "pkgs/swift" },
    { "name": "composer", "path": "pkgs/composer" },
    { "name": "mix", "path": "pkgs/mix" },
    { "name": "dart", "path": "pkgs/dart" },
    { "name": "stack", "path": "pkgs/stack" },
    { "name": "cabal", "path": "pkgs/cabal" },
    { "name": "shrinkwrap", "path": "pkgs/shrinkwrap" },
    { "name": "dotnet-locked", "path": "pkgs/dotnet-locked" },
    { "name": "kotlin", "path": "pkgs/kotlin" },
    { "name": "override-bun", "path": "pkgs/override", "engines": { "bun": ">=1.0.0" } },
    { "name": "composition", "path": "pkgs/composition" },
    { "name": "dual-role", "path": "pkgs/dual-role" }
  ],
  "tasks": {
    "build": { "outputs": ["dist/**"] },
    "dev": { "persistent": true }
  }
}
JSON
lat "$DET" run build --dry-run ; t_ok "detection dry-run resolves all workspaces"
t_has  "detect npm (package-lock.json)"     "npm run build"
t_has  "detect pnpm (pnpm-lock.yaml)"       "pnpm run build"
t_has  "detect yarn (yarn.lock)"            "yarn build"
t_has  "detect bun (bun.lockb)"             "bun run build"
t_has  "detect deno (deno.json)"            "deno task build"
t_has  "detect cargo (Cargo.lock)"          "cargo build"
t_has  "detect go (go.sum)"                 "go build"
t_has  "detect uv (uv.lock)"                "uv run build"
t_has  "detect poetry (poetry.lock)"        "poetry run build"
t_has  "detect bundler (Gemfile.lock)"      "bundle exec build"
t_has  "detect rake (Rakefile)"             "rake build"
t_has  "detect gradle (gradlew)"            "./gradlew build"
t_has  "detect maven (mvnw)"                "./mvnw build"
t_has  "detect dotnet (global.json)"        "dotnet build"
t_has  "detect pdm (pdm.lock)"              "pdm run build"
t_has  "detect pipenv (Pipfile.lock)"       "pipenv run build"
t_has  "detect nuget (packages.config)"     "nuget build"
t_has  "detect pod (Podfile.lock)"          "pod build"
t_has  "detect swift (Package.resolved)"    "swift build"
t_has  "detect composer (composer.lock)"    "composer build"
t_has  "detect mix (mix.lock)"              "mix build"
t_has  "detect dart (pubspec.lock)"         "dart pub build"
t_has  "detect stack (stack.yaml.lock)"     "stack build"
t_has  "detect cabal (cabal.project.freeze)" "cabal build"
t_hasE "detect npm (npm-shrinkwrap.json)"   "shrinkwrap:build.*npm run build"
t_hasE "declared pip drives (requirements.txt)" "pip:build.*pip build"
t_hasE "nuget lockfile leaves dotnet driving"  "dotnet-locked:build.*dotnet build"
t_hasE "kotlin composes under gradle"       "kotlin:build.*\./gradlew build"
t_hasE "declaration overrides lockfile"     "override-bun:build.*bun run build"
t_hasE "roles compose (node+pnpm→pnpm)"     "composition:build.*pnpm run build"
t_hasE "dual-role bun outranks node"        "dual-role:build.*bun run build"

# Persistent tasks are never fabricated for direct-invoke drivers: `run dev`
# resolves only the workspace that declares a `dev` script (npm), never cargo/go.
lat "$DET" run dev --dry-run ; t_ok "persistent-task dry-run exits 0"
t_hasE "persistent dev runs where declared" "npm:dev.*npm run dev"
t_hasnt "persistent dev not fabricated for cargo" "cargo dev"
t_hasnt "persistent dev not fabricated for go"    "go dev"

# =========================================================================
# 11. Error paths & guardrails.
# =========================================================================
sect "error paths & guardrails"

# No config anywhere.
mkdir -p "$ENVROOT/empty"
lat "$ENVROOT/empty" run build ; t_bad "run without lattice.json fails"
t_has "missing-config message" "no lattice.json found"

mkerr() { # mkerr <name> ; sets ERR to the repo dir and creates it
  ERR="$ENVROOT/err/$1"; mkdir -p "$ERR"
}

# Ambiguous: a bare package.json names no tool.
mkerr ambiguity; mkdir -p "$ERR/a"; w "$ERR/a/package.json" '{ "name": "x" }'
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a" } ], "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_bad "ambiguous driver halts"
t_has "ambiguity explains itself"      "ambiguous"
t_has "ambiguity suggests engines fix" "engines"

# Same-role conflict.
mkerr conflict; mkdir -p "$ERR/a"; w "$ERR/a/bun.lockb" ""; w "$ERR/a/pnpm-lock.yaml" ""; w "$ERR/a/package.json" "{}"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a" } ], "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_bad "same-role conflict halts"

# auto:false workspace missing a command for the requested root task.
mkerr manual_nocmd; mkdir -p "$ERR/a"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a", "auto": false, "scripts": { "test": "echo t" } } ], "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_bad "manual workspace missing root command halts"
t_has "manual-missing message" "declares no command"

# Dependency cycle.
mkerr cycle; mkdir -p "$ERR/a"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a", "auto": false, "scripts": { "x": "echo x", "y": "echo y" } } ],
  "tasks": { "x": { "dependsOn": ["y"] }, "y": { "dependsOn": ["x"] } } }
JSON
lat "$ERR" run x ; t_bad "task cycle is rejected"
t_has "cycle message" "cycle detected"

# Persistent task depended upon.
mkerr persist_dep; mkdir -p "$ERR/a"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a", "auto": false, "scripts": { "srv": "sleep 1", "use": "echo use" } } ],
  "tasks": { "srv": { "persistent": true }, "use": { "dependsOn": ["srv"] } } }
JSON
lat "$ERR" run use ; t_bad "depending on a persistent task is rejected"
t_has "persistent-dep message" "cannot be depended on"

# Duplicate workspace name.
mkerr dupname; mkdir -p "$ERR/a" "$ERR/b"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "x", "path": "a" }, { "name": "x", "path": "b" } ], "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_bad "duplicate workspace name is rejected"
t_has "dup-name message" "duplicate workspace name"

# Duplicate workspace path (auto:false so driver detection can't pre-empt it).
mkerr duppath; mkdir -p "$ERR/a"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [
    { "name": "x", "path": "a", "auto": false, "scripts": { "build": "echo x" } },
    { "name": "y", "path": "a", "auto": false, "scripts": { "build": "echo y" } }
  ], "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_bad "duplicate workspace path is rejected"
t_has "dup-path message" "duplicate workspace path"

# Unknown string-form engine.
mkerr badengine
cat > "$ERR/lattice.json" <<'JSON'
{ "engines": { "alpes": ">=2.0.0" }, "workspaces": [], "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_bad "unknown string-form engine is rejected"
t_has "bad-engine suggests versionCmd" "versionCmd"

# Workspace path that isn't a directory.
mkerr notadir
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "gone", "path": "does/not/exist" } ], "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_bad "non-directory workspace path is rejected"
t_has "not-a-dir message" "does not point to a directory"

# Provisioning: installCmd that fails.
mkerr install_fail
cat > "$ERR/lattice.json" <<'JSON'
{ "engines": { "broken": { "version": ">=1.0.0", "versionCmd": "echo 1.0.0", "installCmd": "exit 7" } },
  "workspaces": [], "tasks": { "build": {} } }
JSON
lat "$ERR" setup ; t_bad "failing installCmd fails setup"
t_has "install-fail message" "installCmd failed"

# Validate-only: host tool doesn't satisfy the constraint.
mkerr validate_unsat
cat > "$ERR/lattice.json" <<'JSON'
{ "engines": { "picky": { "version": ">=100.0.0", "versionCmd": "echo 1.0.0" } },
  "workspaces": [], "tasks": { "build": {} } }
JSON
lat "$ERR" setup ; t_bad "unsatisfied version constraint fails setup"
t_has "validate-unsat message" "does not satisfy"

# =========================================================================
# 12. prune & custom cache dir.
# =========================================================================
sect "prune & settings.cacheDir"

# Custom cache directory.
CDIR="$ENVROOT/cachedir"; mkdir -p "$CDIR/pkg"
cat > "$CDIR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "pkg", "path": "pkg", "auto": false, "scripts": { "build": "mkdir -p dist && echo out > dist/o.txt" } } ],
  "tasks": { "build": { "outputs": ["dist/**"] } },
  "settings": { "cacheDir": "custom-cache" } }
JSON
lat "$CDIR" run build ; t_ok "run with custom cacheDir exits 0"
t_file "$CDIR/custom-cache" "settings.cacheDir is honored"
lat "$CDIR" prune --max-size 0B ; t_ok "prune honors custom cacheDir"
t_has "prune reports removal" "removed"

# Prune on the production cache.
lat "$PROD" run build --filter core ; t_ok "re-prime prod cache for prune test"
lat "$PROD" prune --max-size 0B ; t_ok "prune --max-size 0B exits 0"
t_has "prune 0B removes artifacts" "removed"
lat "$PROD" prune ; t_ok "prune with settings.maxCacheSize exits 0"
t_has "prune under limit removes nothing" "removed 0 artifacts"

# Prune with neither flag nor setting.
lat "$DET" prune ; t_bad "prune with no size and no setting fails"
t_has "prune-no-size message" "no max cache size"

# =========================================================================
# 13. Passthrough: a nested repo driven by its own runner.
# =========================================================================
sect "passthrough — nested repo with its own runner"

# A manual workspace whose script shells out to a nested repo's own runner. The
# runner is a stub on PATH that fans a task over the nested packages and leaves a
# nondeterministic marker in its own cache dir — which the `ignore` set must keep
# out of Lattice's key.
NEST="$ENVROOT/nested"
mkdir -p "$NEST/bin" "$NEST/frontend/packages/ui/src" "$NEST/frontend/packages/site/src" "$NEST/api/src"
cat > "$NEST/bin/turbo" <<'SH'
#!/bin/sh
set -e
[ "$1" = "run" ] || { echo "turbo-stub: expected 'run', got '$*'" >&2; exit 2; }
mkdir -p .turbo
echo "$$ $(date +%s)" > .turbo/last-run
for pkg in packages/*; do
  mkdir -p "$pkg/dist"
  cat "$pkg/src/index.js" > "$pkg/dist/bundle.js"
done
echo "turbo-stub: $2 complete"
SH
chmod +x "$NEST/bin/turbo"
w "$NEST/frontend/package.json" '{ "name": "frontend", "private": true }'
w "$NEST/frontend/turbo.json"   '{ "tasks": { "build": {} } }'
w "$NEST/frontend/packages/ui/src/index.js"   "ui v1
"
w "$NEST/frontend/packages/site/src/index.js" "site v1
"
w "$NEST/api/src/main.txt" "api v1
"
cat > "$NEST/lattice.json" <<'JSON'
{
  "workspaces": [
    { "name": "frontend", "path": "frontend", "auto": false,
      "scripts": { "build": "turbo run build" } },
    { "name": "api", "path": "api", "auto": false, "dependsOn": ["frontend"],
      "scripts": { "build": "mkdir -p dist && cp ../frontend/packages/site/dist/bundle.js dist/site.js && echo api-built" } }
  ],
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "inputs": ["**/*"],
      "ignore": ["**/node_modules/**", "**/.turbo/**", "**/dist/**"],
      "outputs": ["dist/**", "packages/*/dist/**"]
    },
    "lint": {}
  }
}
JSON

NESTPATH="PATH=$NEST/bin:$PATH"
late "$NESTPATH" "$NEST" run build -l ; t_ok "passthrough cold run exits 0"
t_has "inner runner was invoked"        "turbo-stub: build complete"
t_has "downstream ran after the nested repo" "api-built"
t_file "$NEST/frontend/packages/site/dist/bundle.js" "inner runner produced its artifacts"
t_nofile "$NEST/.lattice/toolchains" "passthrough repo provisions no toolchains"

late "$NESTPATH" "$NEST" run build -l ; t_ok "passthrough warm run exits 0"
t_has "nested repo caches as one unit" "frontend:build: cache hit"
t_hasnt "a hit never invokes the inner runner" "turbo-stub"

rm -rf "$NEST/frontend/packages/ui/dist" "$NEST/frontend/packages/site/dist"
late "$NESTPATH" "$NEST" run build ; t_ok "passthrough restore run exits 0"
t_file "$NEST/frontend/packages/ui/dist/bundle.js" "hit restored the inner artifacts"

w "$NEST/frontend/packages/ui/src/index.js" "ui v2 CHANGED
"
late "$NESTPATH" "$NEST" run build -l ; t_ok "passthrough run after inner edit exits 0"
t_hasnt "an inner source edit busts the nested key" "frontend:build: cache hit"
t_has   "the busted key re-invokes the inner runner" "turbo-stub: build complete"

# A manual workspace must declare any task invoked directly.
late "$NESTPATH" "$NEST" run lint ; t_bad "root task missing from a manual workspace fails"
t_has "missing-script message names the workspace" "declares no command for task"

# =========================================================================
# 14. Distribution: the installer, `upgrade`, and the pinned-version handover.
# =========================================================================
sect "install, upgrade & version pinning"

# The installer lives in the docs site's public/ directory, which is what serves
# it at latticeandcompany.github.io/lattice/install.sh.
INSTALLER="$REPO_ROOT/apps/web/public/install.sh"
t_file "$INSTALLER" "the installer is where the docs site publishes it"

# A release published to a directory and served over file://. Nothing here
# reaches the network: the URL scheme is the only difference from GitHub, and the
# download, checksum, extract and link path is otherwise identical.
FAKEVER="9.9.9"
TRIPLE="$("$BIN" version --json | sed -n 's/.*"target":"\([^"]*\)".*/\1/p')"
REL="$ENVROOT/release"
STEM="lattice-$FAKEVER-$TRIPLE"
mkdir -p "$REL/v$FAKEVER" "$ENVROOT/relbuild/$STEM/completions"

# The published "binary" identifies itself and echoes its arguments, so a
# handover is observable rather than inferred.
cat > "$ENVROOT/relbuild/$STEM/lattice" <<'SH'
#!/bin/sh
echo "fake-lattice 9.9.9 args: $*"
SH
chmod +x "$ENVROOT/relbuild/$STEM/lattice"
echo "ISC" > "$ENVROOT/relbuild/$STEM/LICENSE"
"$BIN" completions bash > "$ENVROOT/relbuild/$STEM/completions/lattice.bash"
tar -czf "$REL/v$FAKEVER/$STEM.tar.gz" -C "$ENVROOT/relbuild" "$STEM"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
printf '%s  %s\n' "$(sha256_of "$REL/v$FAKEVER/$STEM.tar.gz")" "$STEM.tar.gz" \
  > "$REL/v$FAKEVER/lattice-$FAKEVER-checksums.txt"
printf '{"tag_name":"v%s"}\n' "$FAKEVER" > "$REL/latest.json"

# The CLI takes these as flags. The installer is a shell script piped to sh, so
# it keeps reading the environment — the flags below never reach it.
RELBASE="file://$REL"
RELFLAG="--release-base-url $RELBASE"
LATESTFLAG="--release-latest-url file://$REL/latest.json"
RELENV="LATTICE_RELEASE_BASE_URL=$RELBASE"

# curl built without the file:// protocol cannot serve a local release; the rest
# of the section is meaningless without it.
if curl -V 2>/dev/null | grep -q file; then
  CAN_FETCH=1
else
  CAN_FETCH=0
  say "  ${YEL}skipping local-release checks: curl has no file:// support${RST}"
fi

# --- the installer -------------------------------------------------------
if [ "$CAN_FETCH" = "1" ]; then
  INST="$ENVROOT/installed"
  mkdir -p "$INST"
  w "$INST/.gitignore" "node_modules/
"
  cat > "$INST/lattice.json" <<JSON
{ "latticeVersion": "$FAKEVER", "workspaces": [], "tasks": { "build": {} } }
JSON
  OUTPUT="$(cd "$INST" && env $RELENV sh "$INSTALLER" 2>&1)"; RC=$?
  t_ok    "installer exits 0"
  t_has   "installer names the target triple" "$TRIPLE"
  t_has   "installer verifies the checksum"   "checksum verified"
  t_file  "$INST/.lattice/bin/lattice-$FAKEVER" "installer writes a version-stamped binary"
  t_file  "$INST/.lattice/bin/lattice"          "installer creates the stable path"
  t_grepfile "$INST/.gitignore" ".lattice/bin/" "installer ignores the binaries"
  t_grepfile "$INST/.gitignore" "node_modules/" "installer preserves existing ignores"
  if [ -L "$INST/.lattice/bin/lattice" ] &&
     [ "$(readlink "$INST/.lattice/bin/lattice")" = "lattice-$FAKEVER" ]; then
    pass "stable path is a relative symlink to the versioned binary"
  else
    fail "stable path is a relative symlink to the versioned binary" \
      "$(readlink "$INST/.lattice/bin/lattice" 2>/dev/null)"
  fi
  OUTPUT="$("$INST/.lattice/bin/lattice" run build 2>&1)"; RC=$?
  t_ok  "the installed binary runs"
  t_has "the installed binary is the published one" "fake-lattice 9.9.9"

  # Nothing global: the whole install is one removable directory.
  t_nofile "$INST/lattice" "installer leaves nothing outside .lattice"
  rm -rf "$INST/.lattice"
  t_nofile "$INST/.lattice" "rm -rf .lattice uninstalls completely"

  # Re-running with the version already downloaded must not fetch again.
  cd "$INST" && env $RELENV sh "$INSTALLER" >/dev/null 2>&1
  rm -rf "$REL/v$FAKEVER"
  OUTPUT="$(cd "$INST" && env $RELENV sh "$INSTALLER" 2>&1)"; RC=$?
  t_ok  "installer re-run with the version on disk exits 0"
  t_has "installer reuses the downloaded binary" "already downloaded"
  # Put the release back for the remaining checks.
  mkdir -p "$REL/v$FAKEVER"
  tar -czf "$REL/v$FAKEVER/$STEM.tar.gz" -C "$ENVROOT/relbuild" "$STEM"
  printf '%s  %s\n' "$(sha256_of "$REL/v$FAKEVER/$STEM.tar.gz")" "$STEM.tar.gz" \
    > "$REL/v$FAKEVER/lattice-$FAKEVER-checksums.txt"
fi

# The installer refuses to guess, and refuses to trust.
NOPIN="$ENVROOT/nopin"; mkdir -p "$NOPIN"
w "$NOPIN/lattice.json" '{ "workspaces": [] }'
OUTPUT="$(cd "$NOPIN" && env $RELENV sh "$INSTALLER" 2>&1)"; RC=$?
t_bad "installer fails when lattice.json pins nothing"
t_has "missing-pin message names latticeVersion" "latticeVersion"

if [ "$CAN_FETCH" = "1" ]; then
  BADSUM="$ENVROOT/badsum"; mkdir -p "$BADSUM"
  cp -R "$REL" "$ENVROOT/release-bad"
  printf '%064d  %s\n' 0 "$STEM.tar.gz" \
    > "$ENVROOT/release-bad/v$FAKEVER/lattice-$FAKEVER-checksums.txt"
  cat > "$BADSUM/lattice.json" <<JSON
{ "latticeVersion": "$FAKEVER", "workspaces": [] }
JSON
  OUTPUT="$(cd "$BADSUM" && env "LATTICE_RELEASE_BASE_URL=file://$ENVROOT/release-bad" \
    sh "$INSTALLER" 2>&1)"; RC=$?
  t_bad    "installer fails on a checksum mismatch"
  t_has    "checksum-mismatch message is explicit" "checksum mismatch"
  t_nofile "$BADSUM/.lattice/bin/lattice-$FAKEVER" "a failed checksum installs nothing"
fi

# --- upgrade -------------------------------------------------------------
lat "$ENVROOT" upgrade --help ; t_ok "\`upgrade --help\` exits 0"
t_has "upgrade help documents the version argument" "VERSION"
t_has "upgrade help documents --release-latest-url" "--release-latest-url"
t_has "upgrade help documents --release-list-url"   "--release-list-url"

lat "$DET" upgrade ; t_bad "upgrade requires a version"
lat "$DET" upgrade "../../etc/passwd" ; t_bad "upgrade rejects a path as a version"
t_has "bad-version message" "not a version"

UPG="$ENVROOT/upgrade"; mkdir -p "$UPG"
cat > "$UPG/lattice.json" <<JSON
{
  "\$schema": ".lattice/schema.json",
  "latticeVersion": "$VERSION",
  "workspaces": [],
  "tasks": { "build": {} }
}
JSON
if [ "$CAN_FETCH" = "1" ]; then
  late "" "$UPG" $RELFLAG upgrade "$FAKEVER" ; t_ok "upgrade to a published version exits 0"
  t_has  "upgrade reports the move" "$VERSION"
  t_file "$UPG/.lattice/bin/lattice-$FAKEVER" "upgrade installs the version-stamped binary"
  t_grepfile "$UPG/lattice.json" "\"latticeVersion\": \"$FAKEVER\"" "upgrade rewrites the pin"
  t_grepfile "$UPG/lattice.json" "\"\$schema\"" "upgrade leaves the rest of the config alone"
  t_grepfile "$UPG/lattice.json" "\"build\""    "upgrade leaves the task map alone"

  late "" "$UPG" $RELFLAG upgrade "$FAKEVER" ; t_ok "upgrade is idempotent"
  t_has "upgrade reports an unchanged pin" "already on $FAKEVER"

  late "" "$UPG" $RELFLAG upgrade latest $LATESTFLAG ; t_ok "\`upgrade latest\` exits 0"
  t_has "\`upgrade latest\` resolves a version" "$FAKEVER"

  # The flags replaced environment variables that still work, so an exported
  # value in someone's CI keeps pointing at the same place.
  late "$RELENV" "$UPG" upgrade "$FAKEVER" ; t_ok "LATTICE_RELEASE_BASE_URL still works"
  t_has "the env fallback reaches the same release" "already on $FAKEVER"

  # And where both are given, the flag is the one that counts: the env here
  # points at nothing, so reaching for it would fail the download.
  late "LATTICE_RELEASE_BASE_URL=file://$ENVROOT/nothing-here" "$UPG" \
    $RELFLAG upgrade "$FAKEVER"
  t_ok  "--release-base-url beats LATTICE_RELEASE_BASE_URL"

  # A blank value is not a value; it must fall through rather than build an
  # empty URL out of it.
  late "LATTICE_RELEASE_BASE_URL=" "$UPG" upgrade "not-a-version"
  t_bad "a blank LATTICE_RELEASE_BASE_URL falls through to the default"
  t_has "the blank-value failure is about the version" "not a version"
fi

# --- the handover --------------------------------------------------------
# A binary Lattice installed for the repo, running in a repo that pins a
# different version, must switch rather than run.
PINNED="$ENVROOT/pinned"; mkdir -p "$PINNED/.lattice/bin"
cat > "$PINNED/lattice.json" <<JSON
{ "latticeVersion": "$FAKEVER", "workspaces": [], "tasks": { "build": {} } }
JSON
cp "$BIN" "$PINNED/.lattice/bin/lattice-$VERSION"
ln -sf "lattice-$VERSION" "$PINNED/.lattice/bin/lattice"
MANAGED="$PINNED/.lattice/bin/lattice-$VERSION"

if [ "$CAN_FETCH" = "1" ]; then
  OUTPUT="$(cd "$PINNED" && "$MANAGED" $RELFLAG run build 2>&1)"; RC=$?
  t_ok  "a managed binary in a repo pinned elsewhere exits 0"
  t_has "the handover says which version the repo pins" "this repo pins"
  t_has "the handover says it is switching"             "switching"
  # The whole command line is passed through, the global flag included.
  t_has "the pinned version ran, with the arguments"    "fake-lattice 9.9.9 args: $RELFLAG run build"
  t_file "$PINNED/.lattice/bin/lattice-$FAKEVER" "the handover installed the pinned version"
  if [ "$(readlink "$PINNED/.lattice/bin/lattice")" = "lattice-$FAKEVER" ]; then
    pass "the handover repointed the stable path"
  else
    fail "the handover repointed the stable path" "$(readlink "$PINNED/.lattice/bin/lattice")"
  fi

  # Already on disk: a switch is a symlink swap, so it works with no release at
  # all to fetch from.
  ln -sf "lattice-$VERSION" "$PINNED/.lattice/bin/lattice"
  mv "$REL/v$FAKEVER" "$REL/v$FAKEVER.away"
  OUTPUT="$(cd "$PINNED" && "$MANAGED" $RELFLAG run build 2>&1)"; RC=$?
  t_ok  "switching to a version already on disk exits 0"
  t_has "it is the pinned version that runs" "fake-lattice 9.9.9"
  mv "$REL/v$FAKEVER.away" "$REL/v$FAKEVER"
fi

# Every opt-out leaves the invoked binary running.
OUTPUT="$(cd "$PINNED" && "$MANAGED" --no-version-check run build 2>&1)"; RC=$?
t_ok    "--no-version-check runs the invoked binary"
t_hasnt "--no-version-check prints no notice" "switching"
OUTPUT="$(cd "$PINNED" && env LATTICE_NO_VERSION_CHECK=1 "$MANAGED" run build 2>&1)"; RC=$?
t_ok    "LATTICE_NO_VERSION_CHECK runs the invoked binary"
t_hasnt "LATTICE_NO_VERSION_CHECK prints no notice" "switching"

OFF="$ENVROOT/checkoff"; mkdir -p "$OFF/.lattice/bin"
cat > "$OFF/lattice.json" <<JSON
{ "latticeVersion": "$FAKEVER", "workspaces": [], "tasks": { "build": {} },
  "settings": { "versionCheck": false } }
JSON
cp "$BIN" "$OFF/.lattice/bin/lattice-$VERSION"
OUTPUT="$(cd "$OFF" && "$OFF/.lattice/bin/lattice-$VERSION" run build 2>&1)"; RC=$?
t_ok    "settings.versionCheck false runs the invoked binary"
t_hasnt "settings.versionCheck false prints no notice" "switching"

# A binary Lattice did not install is never replaced: the drift is advice, not an
# action taken on someone else's install.
lat "$PINNED" run build ; t_ok "an unmanaged binary runs in a repo pinned elsewhere"
t_hasnt "an unmanaged binary is never switched" "switching"

# `upgrade`, `version` and `completions` answer for the binary that was invoked,
# so they must not be handed off — with no release published, a handover would
# fail outright.
NOREL="$ENVROOT/norelease"; mkdir -p "$NOREL/.lattice/bin"
cat > "$NOREL/lattice.json" <<JSON
{ "latticeVersion": "$FAKEVER", "workspaces": [], "tasks": { "build": {} } }
JSON
cp "$BIN" "$NOREL/.lattice/bin/lattice-$VERSION"
NOREL_BIN="$NOREL/.lattice/bin/lattice-$VERSION"
OUTPUT="$(cd "$NOREL" && "$NOREL_BIN" --release-base-url "file://$ENVROOT/nothing-here" \
  version --json 2>&1)"; RC=$?
t_ok  "\`version\` is never handed off"
t_has "\`version\` reports the binary that ran" "\"version\":\"$VERSION\""
OUTPUT="$(cd "$NOREL" && "$NOREL_BIN" --release-base-url "file://$ENVROOT/nothing-here" \
  completions bash 2>&1)"; RC=$?
t_ok    "\`completions\` is never handed off"
t_hasnt "completions output stays clean" "switching"

# A pinned version that cannot be fetched is a hard failure: running the wrong
# build silently is the thing this check exists to prevent.
OUTPUT="$(cd "$NOREL" && "$NOREL_BIN" --release-base-url "file://$ENVROOT/nothing-here" \
  run build 2>&1)"; RC=$?
t_bad "an unfetchable pinned version fails loudly"
t_has "the failure names the pinned version" "$FAKEVER"
t_has "the failure names the way out"        "--no-version-check"

# =========================================================================
# 15. The shipped agent skill.
# =========================================================================
# skills/lattice/ is what other people's agents learn Lattice from, so it is
# checked against this binary rather than trusted. A command, a flag, an engine
# name or an invoke template that drifts out of it fails here instead of in
# someone else's repo. Every expectation below is read out of the skill files,
# so updating them is what makes this section pass again.
sect "the shipped agent skill"

SKILLDIR="$REPO_ROOT/skills/lattice"

t_file "$SKILLDIR/SKILL.md"                   "the skill ships at skills/lattice/SKILL.md"
t_file "$SKILLDIR/references/cli.md"          "the skill ships references/cli.md"
t_file "$SKILLDIR/references/toolchains.md"   "the skill ships references/toolchains.md"

# .gitattributes checks Markdown out CRLF, so every field read out of these files
# below would otherwise carry a carriage return. Parse normalized copies.
SKILLN="$ENVROOT/skill"
mkdir -p "$SKILLN/references"
tr -d '\r' < "$SKILLDIR/SKILL.md" > "$SKILLN/SKILL.md"
for ref in "$SKILLDIR"/references/*.md; do
  tr -d '\r' < "$ref" > "$SKILLN/references/$(basename "$ref")"
done
SKILL="$SKILLN/SKILL.md"
SKILLCLI="$SKILLN/references/cli.md"
SKILLTOOLS="$SKILLN/references/toolchains.md"

t_grepfile "$SKILL" "name: lattice" "the skill's frontmatter names it"
t_grepfile "$SKILL" "description:"  "the skill's frontmatter describes it"

for ref in "$SKILLDIR"/references/*.md; do
  base="references/$(basename "$ref")"
  t_grepfile "$SKILL" "$base" "SKILL.md points at $base"
done
for ref in $(grep -oE 'references/[a-z-]+\.md' "$SKILL" | sort -u); do
  t_file "$SKILLDIR/$ref" "the skill ships the $ref it points at"
done

# --- the command surface --------------------------------------------------
lat "$ENVROOT" --help
ALLHELP="$OUTPUT"
BIN_CMDS="$(printf '%s\n' "$OUTPUT" |
  awk '/^Commands:/ { f = 1; next } /^Options:/ { f = 0 } f && NF { print $1 }' | grep -v '^help$')"
SKILL_CMDS="$(sed -n 's/^## `lattice \([a-z][a-z-]*\).*/\1/p' "$SKILLCLI" | sort -u)"

for c in $BIN_CMDS; do
  if printf '%s\n' "$SKILL_CMDS" | grep -qx "$c"; then
    pass "the skill documents \`lattice $c\`"
  else
    fail "the skill documents \`lattice $c\`" "no section for it in references/cli.md"
  fi
done
for c in $SKILL_CMDS; do
  if printf '%s\n' "$BIN_CMDS" | grep -qx "$c"; then
    pass "\`lattice $c\` exists, as the skill says"
  else
    fail "\`lattice $c\` exists, as the skill says" "not in \`lattice --help\`"
  fi
done

# --- the flag surface ----------------------------------------------------
# Both directions: nothing the binary offers goes undocumented, and nothing the
# skill documents is imaginary. `--verbose` is the single exception — a hidden
# alias, documented as hidden, absent from every --help by design.
for c in $BIN_CMDS; do
  lat "$ENVROOT" "$c" --help
  ALLHELP="$ALLHELP
$OUTPUT"
  for f in $(printf '%s\n' "$OUTPUT" | grep -oE '\-\-[a-z][a-z-]+' | sort -u); do
    case "$f" in --help | --version) continue ;; esac
    if grep -qF -- "$f" "$SKILLCLI"; then
      pass "the skill documents \`$c $f\`"
    else
      fail "the skill documents \`$c $f\`" "$f is missing from references/cli.md"
    fi
  done
done
for f in $(grep -ohE '\-\-[a-z][a-z-]+' "$SKILL" "$SKILLCLI" | sort -u); do
  case "$f" in --verbose | --help | --version) continue ;; esac
  if printf '%s\n' "$ALLHELP" | grep -qF -- "$f"; then
    pass "\`$f\` exists, as the skill says"
  else
    fail "\`$f\` exists, as the skill says" "no --help output mentions it"
  fi
done
lat "$ENVROOT" run --not-a-real-flag
if [ "$RC" -eq 2 ]; then pass "a rejected command line exits 2, as the skill says"
else fail "a rejected command line exits 2, as the skill says" "exit=$RC"; fi

# --- the well-known engine list ------------------------------------------
# `--dry-run` against an empty workspace list returns before any version command
# runs, so this checks the config-load verdict on its own — no toolchain needed.
ENGDIR="$ENVROOT/skill-engines"
mkdir -p "$ENGDIR"
eng_config() {
  printf '{ "workspaces": [], "tasks": { "build": {} }, "engines": { "%s": ">=0.0.0" } }' \
    "$1" > "$ENGDIR/lattice.json"
}
SKILL_ENGINES="$(awk -F'|' '
  /^## Well-known engines/ { f = 1; next }
  f && /^## / { f = 0 }
  f && $2 ~ /`/ { gsub(/[` ]/, "", $2); print $2 }
' "$SKILLTOOLS")"
SKILL_DRIVERS="$(awk -F'|' 'NF >= 6 && $2 ~ /`/ { gsub(/[` ]/, "", $2); print $2 }' "$SKILLTOOLS" |
  sort -u)"

for e in $SKILL_ENGINES; do
  eng_config "$e"
  lat "$ENGDIR" run build --dry-run
  t_ok "engine '$e' is accepted in string form, as the skill says"
done
# Anything the skill does not list as well-known has to be rejected in string
# form, or the list has grown and the skill has not.
for e in $(comm -23 <(printf '%s\n' "$SKILL_DRIVERS") <(printf '%s\n' "$SKILL_ENGINES" | sort -u)) alpes; do
  eng_config "$e"
  lat "$ENGDIR" run build --dry-run
  t_bad "engine '$e' is rejected in string form, as the skill says"
  t_has "the rejection of '$e' names versionCmd" "versionCmd"
done

# --- the invoke templates ------------------------------------------------
# Every Build Tool row of the skill's driver table, verified from its own
# fingerprint: these invoke a task directly, with no manifest to consult.
DRVDIR="$ENVROOT/skill-drivers"
mkdir -p "$DRVDIR"
awk -F'|' 'NF >= 6 && $3 ~ /Build Tool/ {
  tool = $2; fps = $4; inv = $6
  gsub(/[` ]/, "", tool)
  gsub(/`/, "", fps); split(fps, a, ","); fp = a[1]; gsub(/^ +| +$/, "", fp)
  gsub(/`/, "", inv); gsub(/^ +| +$/, "", inv)
  print tool "\t" fp "\t" inv
}' "$SKILLTOOLS" > "$DRVDIR/rows.tsv"

DRVWS=""
while IFS=$'\t' read -r tool fp inv; do
  mkdir -p "$DRVDIR/d/$tool"
  : > "$DRVDIR/d/$tool/$fp"
  [ -n "$DRVWS" ] && DRVWS="$DRVWS,"
  DRVWS="$DRVWS{\"name\":\"$tool\",\"path\":\"d/$tool\"}"
done < "$DRVDIR/rows.tsv"
printf '{ "workspaces": [%s], "tasks": { "build": {} } }' "$DRVWS" > "$DRVDIR/lattice.json"

lat "$DRVDIR" run build --dry-run
t_ok "every fingerprint in the skill's driver table resolves a driver"
while IFS=$'\t' read -r tool fp inv; do
  t_has "the skill's invoke template for $tool ($fp)" "$tool:build  ${inv/\{task\}/build}"
done < "$DRVDIR/rows.tsv"

# =========================================================================
# Summary.
# =========================================================================
sect "Summary"
TOTAL=$((PASS + FAIL))
say "ran $TOTAL assertions: ${GRN}$PASS passed${RST}, ${RED}$FAIL failed${RST}"
if [ "$FAIL" -ne 0 ]; then
  say ""
  say "${RED}${BOLD}Failures:${RST}"
  for n in "${FAILED_NAMES[@]}"; do say "  - $n"; done
  say ""
  say "${RED}${BOLD}STRESS TEST FAILED${RST}"
  exit 1
fi
say ""
say "${GRN}${BOLD}ALL FEATURES PASSED${RST}"
exit 0
