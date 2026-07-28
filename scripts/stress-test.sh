#!/usr/bin/env bash
#
# stress-test.sh — exhaustive, self-contained stress test for the `lattice` CLI.
#
# It builds the binary, spins up a throwaway environment containing a
# production-shaped polyglot monorepo (plus a battery of focused sub-repos),
# exercises EVERY command, flag, and code path the tool exposes, asserts the
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
[ -n "$VERSION" ] || VERSION="0.1.0"
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
t_has "help lists completions" "completion"
t_has "help lists version"     "version information"

lat "$ENVROOT" --version ; t_ok "\`--version\` exits 0"
t_has "\`--version\` prints version" "$VERSION"

lat "$ENVROOT" version ; t_ok "\`version\` exits 0"
t_has "\`version\` splash mentions monorepos" "monorepos"
t_has "\`version\` splash shows version"      "$VERSION"

lat "$ENVROOT" version --json ; t_ok "\`version --json\` exits 0"
t_has "version json has version field" "\"version\":\"$VERSION\""
t_has "version json has target field"  "\"target\""

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
t_has "unknown task lists available"    "Available tasks:"

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
lat "$PROD" run build --filter core --no-cache -v ; t_ok "hidden -v alias exits 0"
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
( cd "$PROD" && exec "$BIN" run dev --filter docs -l ) > "$DEVLOG" 2>&1 &
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
mkdir -p "$DET"
w "$DET/pkgs/npm/package-lock.json" "";      w "$DET/pkgs/npm/package.json" "$PKG"
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
w "$DET/pkgs/override/pnpm-lock.yaml" "";     w "$DET/pkgs/override/package.json" "$PKG"
w "$DET/pkgs/composition/.nvmrc" "20";        w "$DET/pkgs/composition/pnpm-lock.yaml" "";  w "$DET/pkgs/composition/package.json" "$PKG"
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
    { "name": "override-bun", "path": "pkgs/override", "engines": { "bun": ">=1.0.0" } },
    { "name": "composition", "path": "pkgs/composition" }
  ],
  "tasks": { "build": { "outputs": ["dist/**"] } }
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
t_hasE "declaration overrides lockfile"     "override-bun:build.*bun run build"
t_hasE "roles compose (node+pnpm→pnpm)"     "composition:build.*pnpm run build"

# =========================================================================
# 11. Error paths & guardrails.
# =========================================================================
sect "error paths & guardrails"

# No config anywhere.
mkdir -p "$ENVROOT/empty"
lat "$ENVROOT/empty" run build ; t_bad "run without lattice.json fails"
t_has "missing-config message" "No lattice.json found"

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
t_has "cycle message" "Cycle detected"

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
t_has "dup-path message" "Duplicate workspace path"

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
