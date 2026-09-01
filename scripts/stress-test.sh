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
# Normalize it. A `TMPDIR` with a trailing slash (the macOS default) leaves a
# `//` in the path, which `cd` collapses — so a test that compares a path it
# built against one a subprocess derived from `$PWD` would compare two spellings
# of the same directory and call them different.
ENVROOT="$(cd "$ENVROOT" && pwd)"
BG_PID=""

cleanup() {
  [ -n "$BG_PID" ] && kill -9 "$BG_PID" 2>/dev/null
  # The stand-in dev server, if a teardown assertion failed and left it behind.
  pkill -f "sleep 3117" 2>/dev/null
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
# Same as `lat`, but killed after <secs> with RC=124. Used for runs that must
# terminate on their own: a regression that waits forever fails one assertion
# instead of hanging the whole suite.
lat_timeout() {
  local dir="$1"; local secs="$2"; shift 2
  local out="$ENVROOT/timed.out"
  : > "$out"
  ( cd "$dir" && "$BIN" "$@" > "$out" 2>&1 ) &
  local pid=$!
  local k=0; local ticks=$((secs * 10))
  while kill -0 "$pid" 2>/dev/null && [ $k -lt $ticks ]; do sleep 0.1; k=$((k + 1)); done
  if kill -0 "$pid" 2>/dev/null; then
    kill -9 "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; RC=124
  else
    wait "$pid"; RC=$?
  fi
  OUTPUT="$(cat "$out")"
}

have()  { printf '%s\n' "$OUTPUT" | grep -qF -- "$1"; }
haveE() { printf '%s\n' "$OUTPUT" | grep -qE -- "$1"; }

t_ok()    { if [ "$RC" -eq 0 ]; then pass "$1"; else fail "$1" "exit=$RC | $(snip)"; fi; }
t_bad()   { if [ "$RC" -ne 0 ]; then pass "$1"; else fail "$1" "expected non-zero exit | $(snip)"; fi; }
# The run ended by itself, rather than being killed by `lat_timeout`.
t_ran()   { if [ "$RC" -ne 124 ]; then pass "$1"; else fail "$1" "timed out | $(snip)"; fi; }
t_has()   { if have  "$2"; then pass "$1"; else fail "$1" "missing [$2] | $(snip)"; fi; }
t_hasE()  { if haveE "$2"; then pass "$1"; else fail "$1" "missing /$2/ | $(snip)"; fi; }
t_hasnt() { if have  "$2"; then fail "$1" "unexpected [$2] | $(snip)"; else pass "$1"; fi; }
t_file()  { if [ -e "$1" ]; then pass "$2"; else fail "$2" "missing file $1"; fi; }
t_nofile(){ if [ -e "$1" ]; then fail "$2" "unexpected file $1"; else pass "$2"; fi; }
t_dir()   { if [ -d "$1" ]; then pass "$2"; else fail "$2" "missing directory $1"; fi; }
t_grepfile() { if grep -qF -- "$2" "$1" 2>/dev/null; then pass "$3"; else fail "$3" "[$2] not in $1"; fi; }
t_nogrepfile() { if grep -qF -- "$2" "$1" 2>/dev/null; then fail "$3" "unexpected [$2] in $1"; else pass "$3"; fi; }

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
t_has "help lists init"        "Create a lattice.json"
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
# 2. init: repo scan, skeleton, artifacts, gitignore, force, guard.
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

# The scan: manifests become workspaces, native version files become engines,
# and dependency/output/gitignored trees are left out of both.
SCANDIR="$ENVROOT/scandir"
mkdir -p "$SCANDIR"
w "$SCANDIR/.gitignore" "generated/
"
w "$SCANDIR/apps/web/package.json" '{ "name": "web", "scripts": { "build": "tsc" } }'
w "$SCANDIR/apps/web/pnpm-lock.yaml" ""
w "$SCANDIR/services/api/Cargo.toml" "[package]
name = \"api\"
"
w "$SCANDIR/services/api/Cargo.lock" ""
w "$SCANDIR/apps/web/node_modules/dep/package.json" '{}'
w "$SCANDIR/dist/package.json" '{}'
w "$SCANDIR/generated/proto/package.json" '{}'
w "$SCANDIR/.nvmrc" "v22.11.0
"
w "$SCANDIR/rust-toolchain.toml" "[toolchain]
channel = \"1.83.0\"
"
lat "$SCANDIR" init --yes ; t_ok "init --yes scans and exits 0"
t_grepfile "$SCANDIR/lattice.json" "apps/web"      "scan declares apps/web"
t_grepfile "$SCANDIR/lattice.json" "services/api"  "scan declares services/api"
t_grepfile "$SCANDIR/lattice.json" "22.11.0"       "scan pins node from .nvmrc"
t_grepfile "$SCANDIR/lattice.json" "1.83.0"        "scan pins rust from rust-toolchain.toml"
t_nogrepfile "$SCANDIR/lattice.json" "node_modules" "scan skips node_modules"
t_nogrepfile "$SCANDIR/lattice.json" "generated"    "scan skips gitignored dirs"
t_nogrepfile "$SCANDIR/lattice.json" "\"dist\""     "scan skips output dirs"
lat "$SCANDIR" run build --dry-run ; t_ok "scanned config drives a real run"
t_has "scanned run plans web" "web"
t_has "scanned run plans api" "api"

# A directory whose driver stays ambiguous is held back, so what init writes
# runs instead of halting on the ambiguity.
UNDRIVEN="$ENVROOT/undriven"
mkdir -p "$UNDRIVEN"
w "$UNDRIVEN/apps/web/package.json" '{ "name": "web", "scripts": { "build": "tsc" } }'
w "$UNDRIVEN/apps/web/package-lock.json" '{}'
w "$UNDRIVEN/crates/core/Cargo.toml" "[package]
name = \"core\"
"
lat "$UNDRIVEN" init --yes ; t_ok "init --yes exits 0 with an undriveable candidate"
t_has "init names the held-back directory" "crates/core"
t_has "init explains the hold-back"        "driver resolved"
t_nogrepfile "$UNDRIVEN/lattice.json" "crates/core" "undriveable candidate is not declared"
t_grepfile   "$UNDRIVEN/lattice.json" "apps/web"    "driveable candidate is declared"
lat "$UNDRIVEN" run build --dry-run ; t_ok "a scanned config never halts on ambiguity"

# A repo whose task runner already declares a pipeline gets that pipeline, not a
# lone `build`: every other task it runs would otherwise be undeclared, and an
# undeclared task is one `lattice run` refuses.
IMPORTED="$ENVROOT/imported"
mkdir -p "$IMPORTED/packages/ui/src"
w "$IMPORTED/package.json" '{ "name": "demo", "private": true }'
w "$IMPORTED/package-lock.json" '{}'
w "$IMPORTED/tsconfig.base.json" '{}'
w "$IMPORTED/turbo.json" '{
  "globalDependencies": ["tsconfig.base.json"],
  "globalEnv": ["CI"],
  "tasks": {
    // turbo.json is read as JSONC, because that is how turbo reads it.
    "build": { "dependsOn": ["^build"], "outputs": ["packages/*/dist/**"] },
    "lint": {},
    "test": { "dependsOn": ["build"] },
    "typecheck": { "dependsOn": ["^build"] },
    "dev": { "persistent": true, "cache": false },
    "ui#deploy": { "dependsOn": ["build"] }
  }
}'
# The tool the tasks run is a dependency of the project, where a package manager
# installs one — not something on the host.
mkdir -p "$IMPORTED/node_modules/.bin"
cat > "$IMPORTED/node_modules/.bin/turbo" <<'SH'
#!/bin/sh
set -e
[ "$1" = "run" ] || { echo "turbo-stub: expected 'run', got '$*'" >&2; exit 2; }
for pkg in packages/*; do
  mkdir -p "$pkg/dist"
  cat "$pkg/src/index.js" > "$pkg/dist/bundle.js"
done
echo "turbo-stub: $2 complete"
SH
chmod +x "$IMPORTED/node_modules/.bin/turbo"
w "$IMPORTED/packages/ui/src/index.js" "ui v1
"
lat "$IMPORTED" init --yes ; t_ok "init --yes exits 0 on a task-runner repo"
t_grepfile "$IMPORTED/lattice.json" '"lint"'      "init imports lint from turbo.json"
t_grepfile "$IMPORTED/lattice.json" '"typecheck"' "init imports typecheck from turbo.json"
t_grepfile "$IMPORTED/lattice.json" '"persistent": true'  "init carries persistent over"
t_grepfile "$IMPORTED/lattice.json" '"cache": false'      "init carries cache over"
t_grepfile "$IMPORTED/lattice.json" 'globalDependencies'  "init imports globalDependencies"
t_grepfile "$IMPORTED/lattice.json" '"CI"'                "init imports globalEnv"
t_nogrepfile "$IMPORTED/lattice.json" "ui#deploy" "a package-scoped entry is not a repo task"

# The reported bug: the runner is installed under node_modules/.bin and nowhere
# else, so a task that names it fails outright unless Lattice puts the project's
# own dependency directory on the task's PATH.
lat "$IMPORTED" run lint ; t_ok "a task finds a tool the project installed"
lat "$IMPORTED" run build ; t_ok "an imported build task runs"
t_file "$IMPORTED/packages/ui/dist/bundle.js" "the project's own runner produced the build"
lat "$IMPORTED" run typecheck ; t_ok "every imported task is runnable, not just build"

# A workspace whose driver takes the task name on its command line publishes no
# list to import, so `build` stays the one proposal.
NOLIST="$ENVROOT/nolist"
mkdir -p "$NOLIST/crates/core"
w "$NOLIST/Cargo.toml" "[workspace]
"
w "$NOLIST/Cargo.lock" ""
lat "$NOLIST" init --yes ; t_ok "init --yes exits 0 without a task list to read"
t_grepfile   "$NOLIST/lattice.json" '"build"' "a driver with no task list still gets build"
t_nogrepfile "$NOLIST/lattice.json" '"lint"'  "and nothing is invented alongside it"

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
      "dev": "echo READY_DEV && sleep 3117"
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
t_has "unknown task lists available"    "Defined tasks:"

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

# Nothing machine-local belongs in the user's source tree: the install marker
# lives under .lattice/, where it is already ignored and out of the fingerprint
# of a task that declares no inputs.
t_nofile "$PROD/libs/core/.lattice-setup-marker"    "setup leaves no marker in a workspace directory"
t_nofile "$PROD/services/api/.lattice-setup-marker" "setup leaves no marker in any workspace directory"

# A name that is not a declared workspace used to select nothing and exit 0, so
# a typo in CI installed nothing and still went green.
lat "$PROD" setup no-such-workspace ; t_bad "setup rejects an undeclared workspace name"
t_has "unknown-workspace names the offender" "no-such-workspace"
t_has "unknown-workspace lists what is declared" "core"

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

# A filter picks the roots of the run: everything they depend on comes with them,
# and nothing that depends on them does. worker → api → core; docs stands alone.
lat "$PROD" run build --filter worker --no-cache ; t_ok "run --filter worker exits 0"
t_has   "filter pulls in a direct dependency"       "api:build"
t_has   "filter pulls in a transitive dependency"   "core:build"
t_hasnt "filter leaves an unrelated workspace out"  "docs:build"

lat "$PROD" run build --dry-run --filter worker ; t_ok "filtered --dry-run exits 0"
t_has   "dry run tags a pulled-in dependency" "core:build (dependency)"
t_hasnt "dry run leaves the match untagged"   "worker:build (dependency)"

# `dev` is only declared by docs, and every other workspace is "auto": false. A
# filtered run must not hold them to a task it never asked them for.
lat "$PROD" run dev --dry-run --filter docs ; t_ok "filtered run skips the task check outside the filter"
t_has "filtered dry run lists the persistent task" "docs:dev"

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

# --no-cache and --force both skip the lookup; only --force writes. The split
# matters: --no-cache leaves a suspect entry in place to be served again, while
# --force is what actually replaces it.
lat "$PROD" run build --filter core --no-cache ; t_hasnt "--no-cache never hits cache" "cache hit"
lat "$PROD" run build --filter core --force    ; t_hasnt "--force never hits cache"    "cache hit"
lat "$PROD" run build --filter core            ; t_has   "--force left a fresh entry behind" "cache hit"

# cache:false task is never cached.
lat "$PROD" run nocache --filter core ; t_ok "nocache run 1 exits 0"
lat "$PROD" run nocache --filter core ; t_ok "nocache run 2 exits 0"
t_hasnt "cache:false task is never a hit" "cache hit"
t_has   "cache:false task always runs"    "core:nocache: running"

# env-keyed cache: same value hits, different value misses.
late "STRESS_VAR=alpha" "$PROD" run envtask --filter core ; t_ok "envtask (alpha) prime exits 0"
late "STRESS_VAR=alpha" "$PROD" run envtask --filter core ; t_has "same env value → cache hit" "cache hit"
late "STRESS_VAR=beta"  "$PROD" run envtask --filter core ; t_hasnt "changed env value → cache miss" "cache hit"

# The stored entry records the value its key was computed from, so an opaque key
# stays explainable.
if grep -qF '"STRESS_VAR": "alpha"' "$PROD"/.lattice/cache/*.meta.json 2>/dev/null; then
  pass "the cache entry records the env value it was keyed on"
else
  fail "the cache entry records the env value it was keyed on" "no meta file names STRESS_VAR=alpha"
fi

# Entries sit directly under cacheDir. The running version is already part of
# every key, so a release that changes what a key covers moves every key on its
# own and needs no second grouping mechanism to retire the old ones.
ENTRIES="$(find "$PROD/.lattice/cache" -maxdepth 1 -name '*.meta.json' 2>/dev/null | wc -l | tr -d ' ')"
if [ "$ENTRIES" -ge 1 ]; then
  pass "cache entries sit directly under the cache directory"
else
  fail "cache entries sit directly under the cache directory" "no *.meta.json at the top of $PROD/.lattice/cache"
fi

NESTED="$(find "$PROD/.lattice/cache" -mindepth 1 -maxdepth 1 -type d -name 'v[0-9]*' 2>/dev/null | wc -l | tr -d ' ')"
if [ "$NESTED" -eq 0 ]; then
  pass "no cache-format directory is created"
else
  fail "no cache-format directory is created" "found a v<n> directory in $PROD/.lattice/cache"
fi

# Full power: a run where every scheduled task came back from cache is called
# out; a partial hit and a run that scheduled nothing are not.
lat "$PROD" run build ; t_ok "run build (whole repo, prime) exits 0"
lat "$PROD" run build ; t_ok "run build (whole repo, all cached) exits 0"
t_has "a fully-cached run is called out" "full power"

# `docs` depends on nothing, so busting it leaves every other task a hit.
w "$PROD/docs/src/page.src" "docs page v1
"
lat "$PROD" run build ; t_ok "run build after leaf edit exits 0"
t_has   "the busted leaf re-ran"                "docs:build"
t_has   "its siblings still hit the cache"      "cache hit"
t_hasnt "a partial hit is not called full power" "full power"
lat "$PROD" run build ; t_has "the leaf edit settles back to full power" "full power"

lat "$PROD" run build --filter nonexistent ; t_hasnt "a run that scheduled nothing is not full power" "full power"

# Saved time. A hit skips the task time the run that wrote the entry spent, so a
# run with hits reports it and `lattice stats` adds those runs up. The ledger is
# per-repo and lives with the cache.
lat "$PROD" run build ; t_has "a fully-cached run reports the time it saved" "saved"
t_file "$PROD/.lattice/cache/stats.jsonl" "the run is recorded in the ledger"
lat "$PROD" stats ; t_ok "stats exits 0"
t_has "stats reports a saved total"        "saved"
t_has "stats reports what it counted"      "runs"
t_has "stats reports the cache it measured" "cache"

# A change in a dependency has to reach the tasks that depend on it. `core` is
# upstream of the apps, so editing it must re-run them too — serving a dependent
# from cache after its dependency rebuilt is how a stale artifact ships.
lat "$PROD" run build ; t_ok "run build settles before the dependency test"
w "$PROD/libs/core/src/lib.src" "core source v3 DEPENDENCY CHANGED
"
lat "$PROD" run build ; t_ok "run build after a dependency edit exits 0"
t_has   "the edited dependency re-ran"            "core:build"
t_hasnt "its dependents did not hit the cache"    "full power"

# Two workspaces must never share an entry. Every cache key names its workspace,
# so a hit in one can't restore the other's artifacts.
CROSSWS="$ENVROOT/cross-ws"
mkdir -p "$CROSSWS/alpha" "$CROSSWS/beta"
w "$CROSSWS/lattice.json" '{
  "workspaces": [
    { "name": "alpha", "path": "alpha", "auto": false, "scripts": { "build": "echo I-AM-ALPHA > out.txt" } },
    { "name": "beta",  "path": "beta",  "auto": false, "scripts": { "build": "echo I-AM-BETA > out.txt" } }
  ],
  "tasks": { "build": { "outputs": ["out.txt"] } }
}'
lat "$CROSSWS" run build ; t_ok "two workspaces sharing a command run"
t_grepfile "$CROSSWS/alpha/out.txt" "I-AM-ALPHA" "alpha kept its own artifact"
t_grepfile "$CROSSWS/beta/out.txt"  "I-AM-BETA"  "beta was not handed alpha's artifact"

# A task with no `inputs` hashes its whole workspace, so a source edit re-runs it.
# It used to hash no files at all and hit forever.
NOINPUTS="$ENVROOT/no-inputs"
mkdir -p "$NOINPUTS/w"
w "$NOINPUTS/lattice.json" '{
  "workspaces": [ { "name": "w", "path": "w", "auto": false, "scripts": { "build": "cat src.txt" } } ],
  "tasks": { "build": {} }
}'
w "$NOINPUTS/w/src.txt" "version one"
lat "$NOINPUTS" run build ; t_ok "undeclared-inputs task primes"
lat "$NOINPUTS" run build ; t_has "undeclared-inputs task hits when nothing changed" "cache hit"
w "$NOINPUTS/w/src.txt" "version two CHANGED"
lat "$NOINPUTS" run build ; t_hasnt "a task with no inputs still notices a source edit" "cache hit"

# A task that declares outputs but produces none is not cached: an empty artifact
# would verify forever, so the task would hit, restore nothing, and never re-run.
EMPTYOUT="$ENVROOT/empty-outputs"
mkdir -p "$EMPTYOUT/w"
w "$EMPTYOUT/lattice.json" '{
  "workspaces": [ { "name": "w", "path": "w", "auto": false, "scripts": { "build": "echo built nothing" } } ],
  "tasks": { "build": { "outputs": ["dist/**"] } }
}'
lat "$EMPTYOUT" run build ; t_ok "a task producing none of its declared outputs still succeeds"
t_has "unmatched outputs are reported rather than cached" "no files matched outputs"
lat "$EMPTYOUT" run build ; t_hasnt "an unmatched-outputs task is not a hit next run" "cache hit"

# The same refusal, reached through the bare-directory form. `dist` matches the
# directory itself, so an empty one used to count as a produced artifact — and a
# hit then restored nothing while deleting whatever a real run had left there.
BAREOUT="$ENVROOT/bare-empty-output"
mkdir -p "$BAREOUT/w"
w "$BAREOUT/lattice.json" '{
  "workspaces": [ { "name": "w", "path": "w", "auto": false, "scripts": { "build": "mkdir -p dist" } } ],
  "tasks": { "build": { "outputs": ["dist"] } }
}'
lat "$BAREOUT" run build ; t_ok "a bare-directory output that stays empty still succeeds"
t_has "an empty output directory is not cached" "matched only empty directories"
lat "$BAREOUT" run build ; t_hasnt "an empty-directory output is not a hit next run" "cache hit"

# A bare directory that actually holds something is still captured whole.
w "$BAREOUT/lattice.json" '{
  "workspaces": [ { "name": "w", "path": "w", "auto": false, "scripts": { "build": "mkdir -p dist/deep && echo out > dist/deep/o.txt" } } ],
  "tasks": { "build": { "outputs": ["dist"] } }
}'
lat "$BAREOUT" run build ; t_ok "a bare-directory output with content primes"
rm -rf "$BAREOUT/w/dist"
lat "$BAREOUT" run build ; t_has "a bare-directory output hits" "cache hit"
t_file "$BAREOUT/w/dist/deep/o.txt" "a bare-directory output restores its subtree"

# Restoring a hit reproduces the tree the run produced, rather than leaving
# directories behind that the cached run never made.
mkdir -p "$BAREOUT/w/dist/stale"
w "$BAREOUT/w/dist/stale/extra.txt" "left over from a later run"
lat "$BAREOUT" run build ; t_has "restore-over-stale is a hit" "cache hit"
t_nofile "$BAREOUT/w/dist/stale" "a hit clears directories the cached run did not produce"

# Inputs reached through a symlink. The walk used to stop at any symlink, so a
# workspace whose sources were symlinked hashed nothing and hit forever.
LN_OK=1
( cd "$ENVROOT" && ln -s . lncheck ) 2>/dev/null || LN_OK=0
rm -f "$ENVROOT/lncheck"
if [ "$LN_OK" = "1" ]; then
  SYMIN="$ENVROOT/symlinked-inputs"
  mkdir -p "$SYMIN/w" "$SYMIN/real/nested"
  w "$SYMIN/real/nested/a.txt" "one"
  ( cd "$SYMIN/w" && ln -s ../real src )
  w "$SYMIN/lattice.json" '{
    "workspaces": [ { "name": "w", "path": "w", "auto": false, "scripts": { "build": "echo built" } } ],
    "tasks": { "build": { "inputs": ["src/**"] } }
  }'
  lat "$SYMIN" run build ; t_ok "a task whose inputs are behind a symlink primes"
  lat "$SYMIN" run build ; t_has "unchanged symlinked inputs hit" "cache hit"
  w "$SYMIN/real/nested/a.txt" "two CHANGED"
  lat "$SYMIN" run build ; t_hasnt "an edit behind a symlinked directory busts the cache" "cache hit"

  # Re-pointing a symlinked file is a change to what the task reads, so the key
  # has to move even though no file contents changed.
  SYMFILE="$ENVROOT/symlink-swap"
  mkdir -p "$SYMFILE/w"
  w "$SYMFILE/w/prod.yaml" "prod"
  w "$SYMFILE/w/staging.yaml" "staging"
  ( cd "$SYMFILE/w" && ln -s prod.yaml active.yaml )
  w "$SYMFILE/lattice.json" '{
    "workspaces": [ { "name": "w", "path": "w", "auto": false, "scripts": { "build": "echo built" } } ],
    "tasks": { "build": { "inputs": ["active.yaml"] } }
  }'
  lat "$SYMFILE" run build ; t_ok "a symlinked input file primes"
  lat "$SYMFILE" run build ; t_has "an unmoved symlink hits" "cache hit"
  ( cd "$SYMFILE/w" && ln -sf staging.yaml active.yaml )
  lat "$SYMFILE" run build ; t_hasnt "re-pointing a symlink busts the cache" "cache hit"

  # The executable bit rides along in the artifact, so it belongs in the key.
  EXECBIT="$ENVROOT/exec-bit"
  mkdir -p "$EXECBIT/w/bin"
  w "$EXECBIT/w/bin/run.sh" "#!/bin/sh
echo hi
"
  w "$EXECBIT/lattice.json" '{
    "workspaces": [ { "name": "w", "path": "w", "auto": false, "scripts": { "build": "echo packaged" } } ],
    "tasks": { "build": { "inputs": ["bin/**"] } }
  }'
  lat "$EXECBIT" run build ; t_ok "a task keyed on a bin directory primes"
  lat "$EXECBIT" run build ; t_has "unchanged mode hits" "cache hit"
  chmod +x "$EXECBIT/w/bin/run.sh"
  lat "$EXECBIT" run build ; t_hasnt "making an input executable busts the cache" "cache hit"
fi

# =========================================================================
# 7. run: PATH injection, concurrency, loquacious, other tasks.
# =========================================================================
sect "run — PATH injection, concurrency, verbosity, other tasks"

lat "$PROD" run build --filter api --no-cache ; t_ok "run build api exits 0"
t_grepfile "$PROD/services/api/dist/api.txt" "faketool 1.4.2" "provisioned tool resolved via injected PATH"

lat "$PROD" run build --no-cache --concurrency 1 ; t_ok "run --concurrency 1 exits 0"
lat "$PROD" run build --no-cache --concurrency 4 ; t_ok "run --concurrency 4 exits 0"

lat "$PROD" run build --filter core --no-cache -v ; t_ok "run -v (loquacious) exits 0"
t_has "loquacious emits hash trace" "hash"
t_hasnt "piped -v emits no ANSI escapes" "$TRUECOLOR"
lat "$PROD" run build --filter core --no-cache -l ; t_ok "hidden -l alias exits 0"

# Label colors: `-v` at a real terminal paints each `workspace:task` label its
# own color, and every task in the run gets a different one. Piped (asserted
# above) and under NO_COLOR, the same run emits nothing to strip.
if [ "$PTY_OK" = "1" ]; then
  ptylat "" "$PROD" run build --no-cache -v
  t_has "-v under a pty colors the workspace:task label" "$TRUECOLOR"
  HUES="$(printf '%s\n' "$OUTPUT" | grep -o "$TRUECOLOR"'[0-9;]*m' | sort -u | wc -l | tr -d ' ')"
  if [ "${HUES:-0}" -ge 2 ]; then
    pass "each task's label gets a distinct color ($HUES in the run)"
  else
    fail "each task's label gets a distinct color" "only $HUES distinct | $(snip)"
  fi
  ptylat "NO_COLOR=1" "$PROD" run build --no-cache -v
  t_hasnt "NO_COLOR suppresses label color under a pty" "$TRUECOLOR"
else
  say "  ${YEL}skip${RST} label-color assertions (no \`script\` to allocate a pty)"
fi

lat "$PROD" run build --filter core --no-cache --no-version-check ; t_ok "--no-version-check accepted"

# -v so the CI reporter streams task output (echoed markers) we assert on.
lat "$PROD" run test  --no-cache -v ; t_ok "run test (full) exits 0"
t_has "test ran downstream of build" "core-test-ok"
lat "$PROD" run lint  -v ; t_ok "run lint exits 0"
t_has "lint ran"  "core-lint-ok"
lat "$PROD" run clean -v ; t_ok "run clean exits 0"
t_has "clean ran" "core-clean-ok"

# Stacked commands: one invocation, one combined graph. lint + test + build run
# together; test's build dependency runs once, ahead of test.
lat "$PROD" run lint test build --no-cache -v ; t_ok "run lint test build (stacked) exits 0"
t_has "stacked run ran lint"  "core-lint-ok"
t_has "stacked run ran test"  "core-test-ok"
t_has "stacked run built"     "core:build"
lat "$PROD" run lint test build --dry-run ; t_ok "stacked --dry-run exits 0"
t_has "stacked dry-run banner lists all roots" "dry run · lint test build"
lat "$PROD" run build definitely-not-a-task ; t_bad "stacked run rejects an unknown task"
t_has "unknown stacked task names the offender" "definitely-not-a-task"

# --sequentially: each task's graph runs to completion before the next, in order.
lat "$PROD" run lint test -s --no-cache -v ; t_ok "run lint test --sequentially exits 0"
t_has "sequential run ran lint"  "core-lint-ok"
t_has "sequential run ran test"  "core-test-ok"
lat "$PROD" run lint build --sequentially --dry-run ; t_ok "sequential --dry-run exits 0"
t_has "sequential dry-run labels the lint phase"  "dry run · lint (phase)"
t_has "sequential dry-run labels the build phase" "dry run · build (phase)"

# =========================================================================
# 8. Persistent task (dev server): must not block; SIGINT tears down; an exit
#    of its own is reported.
# =========================================================================
sect "persistent tasks"

DEVLOG="$ENVROOT/dev.log"
: > "$DEVLOG"
# No -v: a run that pulls in a persistent task auto-selects raw, line-by-line
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
# A child we killed on purpose is not an exit to report and not a failure.
if grep -q "EXITED" "$DEVLOG" 2>/dev/null; then fail "SIGINT teardown reports no exit" "$(tr '\n' '|' < "$DEVLOG" | cut -c1-300)"; else pass "SIGINT teardown reports no exit"; fi
t_grepfile "$DEVLOG" "0 failed" "interrupted persistent run reports no failures"
if pgrep -f "sleep 3117" >/dev/null 2>&1; then fail "SIGINT kills the dev server's process group" "the dev server's sleep survived"; else pass "SIGINT kills the dev server's process group"; fi

# A `persistent: true` command that exits anyway is noticed: the run reports it
# and ends on its own, with no signal involved.
EXITREPO="$ENVROOT/exitrepo"
mkdir -p "$EXITREPO/app"
cat > "$EXITREPO/lattice.json" <<'JSON'
{
  "workspaces": [
    { "name": "app", "path": "app", "auto": false, "scripts": {
      "dev":  "echo PORT_ALREADY_IN_USE; exit 1",
      "once": "echo ONE_SHOT_OK"
    } }
  ],
  "tasks": {
    "dev":  { "persistent": true },
    "once": { "persistent": true }
  }
}
JSON
lat_timeout "$EXITREPO" 20 run dev
t_ran  "a persistent task that exits ends the run without a signal"
t_bad  "a persistent task exiting non-zero fails the run"
t_has  "persistent exit names the code"    "EXITED (code 1)"
t_has  "persistent exit counts as failed"  "1 failed"
t_has  "persistent output still streamed"  "PORT_ALREADY_IN_USE"

lat_timeout "$EXITREPO" 20 run once
t_ran  "a clean persistent exit ends the run too"
t_ok   "a persistent task exiting 0 does not fail the run"
t_has  "clean persistent exit is reported" "exited (code 0)"
t_has  "clean persistent exit counts none" "0 failed"

# Fail-fast has to reach a run a dev server is holding open. The wait for a
# persistent child used to ignore the abort the failure had already raised, so
# the run sat there streaming until someone interrupted it by hand.
FFREPO="$ENVROOT/failfast-persistent"
mkdir -p "$FFREPO/app"
cat > "$FFREPO/lattice.json" <<'JSON'
{
  "workspaces": [
    { "name": "app", "path": "app", "auto": false, "scripts": {
      "build": "echo BUILD_BROKE; exit 2",
      "dev":   "echo READY_FF; sleep 3119"
    } }
  ],
  "tasks": { "build": {}, "dev": { "persistent": true } }
}
JSON
lat_timeout "$FFREPO" 25 run build dev
t_ran "a failure ends a run that a persistent task is holding open"
t_bad "that run still reports the failure"
t_has "the failing task is named" "BUILD_BROKE"
if pgrep -f "sleep 3119" >/dev/null 2>&1; then fail "fail-fast kills the dev server it started" "the dev server's sleep survived"; else pass "fail-fast kills the dev server it started"; fi

# The signal a CI runner sends when a job is cancelled is SIGTERM, not SIGINT.
# It used to be watched everywhere except the wait that actually holds the run
# open, so a cancelled job hung until the runner force-killed it.
TERMLOG="$ENVROOT/dev-term.log"
: > "$TERMLOG"
( cd "$PROD" && exec "$BIN" run dev --filter docs ) > "$TERMLOG" 2>&1 &
BG_PID=$!
i=0
while [ $i -lt 150 ]; do
  grep -q "READY_DEV" "$TERMLOG" 2>/dev/null && break
  kill -0 "$BG_PID" 2>/dev/null || break
  sleep 0.1; i=$((i + 1))
done
kill -TERM "$BG_PID" 2>/dev/null
j=0
while kill -0 "$BG_PID" 2>/dev/null && [ $j -lt 60 ]; do sleep 0.1; j=$((j + 1)); done
if kill -0 "$BG_PID" 2>/dev/null; then
  kill -9 "$BG_PID" 2>/dev/null; wait "$BG_PID" 2>/dev/null
  fail "persistent run terminates on SIGTERM" "still running after 6s — hung"
else
  wait "$BG_PID" 2>/dev/null
  pass "persistent run terminates on SIGTERM"
fi
BG_PID=""

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
t_has "fail-fast surfaces the failure with the exit code" "FAILED (code 3)"

# The live display is the surface the trace lines used to leak into: the full
# hash and the miss reason printed dim above a task whose own line already said
# both. Only a pty renders it, so only a pty can assert they are gone.
if [ "$PTY_OK" = "1" ]; then
  ptylat "" "$FAILREPO" run build
  t_has   "the live display names the exit code"        "FAILED (code 3)"
  t_hasnt "the live display carries no hash trace"      "hash "
  t_hasnt "the live display carries no cache-miss trace" "cache miss"
else
  say "  ${YEL}skip${RST} live-display assertions (no \`script\` to allocate a pty)"
fi

# Output was read as text a line at a time, and a byte that would not decode
# read as end-of-output — so a tool that printed one stray byte lost everything
# after it, including the error that explained the failure.
ODDBYTE="$ENVROOT/odd-byte"
mkdir -p "$ODDBYTE/w"
cat > "$ODDBYTE/lattice.json" <<'JSON'
{
  "workspaces": [ { "name": "w", "path": "w", "auto": false, "scripts": {
    "build": "printf 'warning \\377 here\\n'; echo THE_REAL_ERROR; exit 1"
  } } ],
  "tasks": { "build": {} }
}
JSON
lat "$ODDBYTE" run build ; t_bad "a task printing an undecodable byte still fails"
t_has "output survives a byte that will not decode" "THE_REAL_ERROR"

# -v so skip notices and streamed output are visible for assertions.
lat "$FAILREPO" run test --continue -v ; t_bad "--continue still exits non-zero when a task failed"
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
# deno.jsonc allows comments. Parsed as strict JSON it fails, and a failed parse
# used to be indistinguishable from "this driver has no manifest" — which meant
# every task got a command invented for it.
w "$DET/pkgs/deno-jsonc/deno.jsonc" '{
  // the tasks this package actually declares
  "tasks": { "build": "echo x" }
}'
# A manifest with no scripts at all is a complete, ordinary package. The task
# simply does not exist here, so it drops out of the graph instead of becoming
# an invented `npm run build` that fails the whole run.
w "$DET/pkgs/no-scripts/package-lock.json" ""; w "$DET/pkgs/no-scripts/package.json" '{ "name": "types" }'
# A manifest that declares a scripts section but misspells the task is the case
# worth saying something about: the package clearly meant to build.
w "$DET/pkgs/typo-script/package-lock.json" ""; w "$DET/pkgs/typo-script/package.json" '{ "name": "typo", "scripts": { "biuld": "tsc" } }'
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
    { "name": "dual-role", "path": "pkgs/dual-role" },
    { "name": "deno-jsonc", "path": "pkgs/deno-jsonc" },
    { "name": "no-scripts", "path": "pkgs/no-scripts" },
    { "name": "typo-script", "path": "pkgs/typo-script" }
  ],
  "tasks": {
    "build": { "outputs": ["dist/**"] },
    "dev": { "persistent": true }
  }
}
JSON
lat "$DET" run build --dry-run ; t_ok "detection dry-run resolves all workspaces"
t_has   "a commented deno.jsonc still resolves its tasks" "deno-jsonc:build"
t_hasnt "a manifest with no scripts invents no command"   "no-scripts:build"
t_hasnt "a manifest with no scripts invents nothing for typos either" "typo-script:build"
# The skip is silent for a package that declares no scripts at all, and spoken
# for one that declares some and misspelled this one.
t_has   "a misspelled script is named once"        'typo-script declares scripts but no "build"'
t_has   "a misspelled script gets a suggestion"    "biuld"
t_hasnt "a scriptless package is not nagged about" "no-scripts declares"
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

# A runtime cannot drive tasks, so the fix offered here must not name one --
# pasting it in has to resolve the halt rather than reproduce it.
mkerr runtime_only; mkdir -p "$ERR/a"; w "$ERR/a/.nvmrc" "20"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a" } ], "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_bad "runtime-only workspace halts"
t_has "runtime-only suggests scripts, not engines" "scripts"
t_hasnt "runtime-only does not suggest a runtime" '"node"'

# ...and the suggested fix actually works.
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a", "auto": false,
    "scripts": { "build": "echo built" } } ], "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_ok "the suggested scripts fix resolves the halt"

# Same-role conflict.
mkerr conflict; mkdir -p "$ERR/a"; w "$ERR/a/bun.lockb" ""; w "$ERR/a/pnpm-lock.yaml" ""; w "$ERR/a/package.json" "{}"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a" } ], "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_bad "same-role conflict halts"

# auto:false workspace missing a command for the requested root task.
mkerr manual_nocmd; mkdir -p "$ERR/a"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a", "auto": false, "scripts": { "test": "echo t" } } ], "tasks": { "build": {}, "test": {} } }
JSON
lat "$ERR" run build ; t_bad "manual workspace missing root command halts"
t_has "manual-missing message" "declares no command"

# A script naming a task that does not exist is unreachable, so it is refused
# rather than silently never running.
mkerr manual_stray; mkdir -p "$ERR/a"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a", "auto": false, "scripts": { "biuld": "echo b" } } ], "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_bad "a script naming no task is refused"
t_has "stray-script message"    "is not defined in \`tasks\`"
t_has "stray-script suggestion" "Did you mean"

# A repeated key is last-wins in JSON, so the first declaration vanishes — and
# with it whatever it said about what the task hashes and caches.
mkerr dupkey; mkdir -p "$ERR/a"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a", "auto": false, "scripts": { "build": "echo b" } } ],
  "tasks": { "build": { "outputs": ["dist/**"] }, "build": {} } }
JSON
lat "$ERR" run build ; t_bad "a duplicate task key is refused"
t_has "duplicate-key message" "duplicate key \`build\`"

# An out-of-range timeout used to saturate into no timeout at all — the opposite
# of what was asked for.
mkerr bigtimeout; mkdir -p "$ERR/a"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a", "auto": false, "scripts": { "build": "echo b" } } ],
  "tasks": { "build": { "timeout": 1e30 } } }
JSON
lat "$ERR" run build ; t_bad "an oversized timeout is refused"
t_has "oversized-timeout message" "maximum of 365 days"

mkerr fractimeout; mkdir -p "$ERR/a"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a", "auto": false, "scripts": { "build": "echo b" } } ],
  "tasks": { "build": { "timeout": 1.5 } } }
JSON
lat "$ERR" run build ; t_bad "a fractional timeout is refused rather than rounded"
t_has "fractional-timeout message" "whole number of seconds"

# An engine's bin is joined onto the toolchain directory, so an absolute value
# would put a host directory on every task's PATH while claiming a pinned tool.
mkerr enginebin; mkdir -p "$ERR/a"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a", "auto": false, "scripts": { "build": "echo b" } } ],
  "tasks": { "build": {} },
  "engines": { "alpes": { "installCmd": "true", "bin": "/usr/bin" } } }
JSON
lat "$ERR" run build ; t_bad "an absolute engine bin is refused"
t_has "engine-bin message" "toolchain"

# Dependency cycle.
mkerr cycle; mkdir -p "$ERR/a"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a", "auto": false, "scripts": { "x": "echo x", "y": "echo y" } } ],
  "tasks": { "x": { "dependsOn": ["y"] }, "y": { "dependsOn": ["x"] } } }
JSON
lat "$ERR" run x ; t_bad "task cycle is rejected"
t_has "cycle message" "the task graph has a cycle"

# Persistent task depended upon.
mkerr persist_dep; mkdir -p "$ERR/a"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a", "auto": false, "scripts": { "srv": "sleep 1", "use": "echo use" } } ],
  "tasks": { "srv": { "persistent": true }, "use": { "dependsOn": ["srv"] } } }
JSON
lat "$ERR" run use ; t_bad "depending on a persistent task is rejected"
t_has "persistent-dep message" "no other task may depend on it"

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

# An unknown key is refused rather than ignored. A `output` written for `outputs`
# would otherwise decide, silently, what the task caches.
mkerr unknown_top
cat > "$ERR/lattice.json" <<'JSON'
{
  "projects": {},
  "workspaces": [],
  "tasks": { "build": {} }
}
JSON
lat "$ERR" run build ; t_bad "an unknown top-level key is rejected"
t_has "unknown-key message names the key"      'unknown field `projects`'
t_has "unknown-key message places the key"     "at the top level of lattice.json"
t_has "unknown-key message gives the position" "line 2"
t_has "unknown-key message lists the fields"   "Fields accepted here:"

# Inside a task, with a near miss to point at.
mkerr unknown_task
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [], "tasks": { "build": { "output": ["dist/**"] } } }
JSON
lat "$ERR" run build ; t_bad "a misspelled outputs key is rejected"
t_has "the typo is placed in its task" 'unknown field `output` in tasks.build'
t_has "the typo gets a suggestion"     'Did you mean `outputs`?'

# Inside a workspace entry, which is indexed so the right one gets read.
mkerr unknown_ws; mkdir -p "$ERR/a" "$ERR/b"
cat > "$ERR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "a", "path": "a" },
                  { "name": "b", "path": "b", "dependOn": ["a"] } ],
  "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_bad "an unknown workspace key is rejected"
t_has "the offending workspace is indexed"   'unknown field `dependOn` in workspaces[1]'
t_has "the workspace typo gets a suggestion" 'Did you mean `dependsOn`?'

# An engine object reports its own unknown key, rather than the enclosing
# either/or reporting that neither form matched.
mkerr unknown_engine
cat > "$ERR/lattice.json" <<'JSON'
{ "engines": { "node": { "versionCmnd": "node --version" } },
  "workspaces": [], "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_bad "an unknown engine-object key is rejected"
t_has "the engine typo is placed"          'unknown field `versionCmnd` in engines.node'
t_has "the engine typo gets a suggestion"  'Did you mean `versionCmd`?'

# ...and a value in neither engine form names both.
mkerr engine_type
cat > "$ERR/lattice.json" <<'JSON'
{ "engines": { "node": 20 }, "workspaces": [], "tasks": { "build": {} } }
JSON
lat "$ERR" run build ; t_bad "an engine value in neither form is rejected"
t_has "the engine-type message names both forms" "version constraint string or an engine object"

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

# A retired setting is a parse error like any other unknown key: `logging` was
# removed, and a config still carrying it has to be told so rather than run.
cat > "$CDIR/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "pkg", "path": "pkg", "auto": false, "scripts": { "build": "mkdir -p dist && echo out > dist/o.txt" } } ],
  "tasks": { "build": { "outputs": ["dist/**"] } },
  "settings": { "cacheDir": "custom-cache", "logging": "debug" } }
JSON
lat "$CDIR" run build --no-cache ; t_bad "a config carrying the retired settings.logging is rejected"
t_has "the retired setting is named" 'unknown field `logging` in settings'

# Prune on the production cache.
lat "$PROD" run build --filter core ; t_ok "re-prime prod cache for prune test"
lat "$PROD" prune --max-size 0B ; t_ok "prune --max-size 0B exits 0"
t_has "prune 0B removes artifacts" "removed"
lat "$PROD" prune ; t_ok "prune with settings.maxCacheSize exits 0"
t_has "prune under limit removes nothing" "removed 0 artifacts"

# Prune with neither flag nor setting.
lat "$DET" prune ; t_bad "prune with no size and no setting fails"
t_has "prune-no-size message" "no cache size limit set"

# Debris is only reclaimed once it is old enough to be nothing else. A cache
# write records its metadata first and its archive second, so a store running
# right now in another process looks exactly like an abandoned one on disk —
# sweeping eagerly meant two Lattice runs on one repo deleted each other's work.
SWEEP="$ENVROOT/sweep-grace"; mkdir -p "$SWEEP/pkg/.cache"
cat > "$SWEEP/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "pkg", "path": "pkg", "auto": false, "scripts": { "build": "mkdir -p dist && echo out > dist/o.txt" } } ],
  "tasks": { "build": { "outputs": ["dist/**"] } },
  "settings": { "cacheDir": ".lattice/cache" } }
JSON
lat "$SWEEP" run build ; t_ok "sweep fixture primes its cache"
w "$SWEEP/.lattice/cache/c0ffee.tar.gz" "an artifact being written right now"
w "$SWEEP/.lattice/cache/deadbeef.tar.gz" "an artifact nobody is coming back for"
touch -t 202001010000 "$SWEEP/.lattice/cache/deadbeef.tar.gz"
lat "$SWEEP" prune --max-size 1GB ; t_ok "prune over a generous budget exits 0"
t_file   "$SWEEP/.lattice/cache/c0ffee.tar.gz"   "prune leaves a leftover new enough to be a live write"
t_nofile "$SWEEP/.lattice/cache/deadbeef.tar.gz" "prune still reclaims an abandoned leftover"

# cacheDir names a directory prune deletes inside, so it has to stay somewhere
# of its own inside the repo.
badcache() {
  BADC="$ENVROOT/badcache"; rm -rf "$BADC"; mkdir -p "$BADC/pkg"
  cat > "$BADC/lattice.json" <<JSON
{ "workspaces": [ { "name": "pkg", "path": "pkg", "auto": false, "scripts": { "build": "echo b" } } ],
  "tasks": { "build": {} },
  "settings": { "cacheDir": "$1" } }
JSON
  lat "$BADC" run build
}
badcache "/tmp/lattice-stress-absolute" ; t_bad "an absolute cacheDir is refused"
t_has "absolute cacheDir message" "not relative to the repo root"
badcache "../outside" ; t_bad "a cacheDir above the repo is refused"
t_has "escaping cacheDir message" "outside the repo root"
badcache "." ; t_bad "a cacheDir that is the repo root is refused"
t_has "repo-root cacheDir message" "the repo root itself"

# =========================================================================
# 12b. Correctness guardrails: the failures that used to be silent.
# =========================================================================
sect "guardrails — shared files, references, signals, budgets, timeouts"

# --- globalDependencies: a file above the workspace ------------------------
# `inputs` is workspace-relative and `tasks` is shared across workspaces, so a
# shared root file has no `inputs` spelling that means the same thing in each.
# Without globalDependencies nothing covered it, and editing one served every
# task an artifact built before the change.
GDEP="$ENVROOT/globaldeps"; mkdir -p "$GDEP/app"
w "$GDEP/shared.config.json" '{"mode":"one"}'
cat > "$GDEP/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "app", "path": "app", "auto": false,
                    "scripts": { "build": "cat ../shared.config.json > out.txt" } } ],
  "globalDependencies": ["shared.config.json"],
  "tasks": { "build": { "outputs": ["out.txt"] } } }
JSON
lat "$GDEP" run build ; t_ok "globalDependencies: prime run exits 0"
lat "$GDEP" run build ; t_has "globalDependencies: untouched run still hits" "cache"
w "$GDEP/shared.config.json" '{"mode":"TWO"}'
lat "$GDEP" run build ; t_ok "globalDependencies: run after edit exits 0"
t_hasnt "editing a shared root file busts every key" "cache hit"
t_grepfile "$GDEP/app/out.txt" '"mode":"TWO"' "the restored output is not the pre-edit one"

# The miss names the component that moved, rather than reporting a bare miss.
w "$GDEP/shared.config.json" '{"mode":"three"}'
lat "$GDEP" -v run build ; t_has "a miss names globalDependencies as the cause" "globalDependencies changed"

# --- globalEnv -------------------------------------------------------------
GENV="$ENVROOT/globalenv"; mkdir -p "$GENV/app"
cat > "$GENV/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "app", "path": "app", "auto": false,
                    "scripts": { "build": "echo built > out.txt" } } ],
  "globalEnv": ["STRESS_GLOBAL"],
  "tasks": { "build": { "outputs": ["out.txt"] } } }
JSON
late "STRESS_GLOBAL=one" "$GENV" run build ; t_ok "globalEnv: prime run exits 0"
late "STRESS_GLOBAL=one" "$GENV" run build ; t_has "globalEnv: same value hits"  "cache hit"
late "STRESS_GLOBAL=two" "$GENV" run build ; t_hasnt "globalEnv: changed value misses" "cache hit"
late "STRESS_GLOBAL=two" "$GENV" -v run build --force ; t_ok "globalEnv: --force run exits 0"

# --- a miss with nothing to compare against --------------------------------
NEVER="$ENVROOT/neverrun"; mkdir -p "$NEVER/app"
cat > "$NEVER/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "app", "path": "app", "auto": false,
                    "scripts": { "build": "echo hi > out.txt" } } ],
  "tasks": { "build": { "outputs": ["out.txt"] } } }
JSON
lat "$NEVER" -v run build ; t_has "a task that never ran says so instead of naming a component" "nothing cached"

# --- dependsOn that names nothing ------------------------------------------
# Both used to build no edge at all, so the ordering the config was written to
# guarantee simply did not happen, with nothing printed.
BADREF="$ENVROOT/badref"; mkdir -p "$BADREF/lib" "$BADREF/app"
cat > "$BADREF/lattice.json" <<'JSON'
{ "workspaces": [
    { "name": "lib", "path": "lib", "auto": false, "scripts": { "build": "echo lib" } },
    { "name": "app", "path": "app", "auto": false, "dependsOn": ["libb"],
      "scripts": { "build": "echo app" } } ],
  "tasks": { "build": { "dependsOn": ["^build"] } } }
JSON
lat "$BADREF" run build --dry-run ; t_bad "a workspace dependsOn naming nothing is rejected"
t_has "the unresolvable workspace name is named" "not a declared workspace"
t_has "the nearest workspace name is offered"    "Did you mean \`lib\`?"

cat > "$BADREF/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "app", "path": "app", "auto": false, "scripts": { "build": "echo app" } } ],
  "tasks": { "build": { "dependsOn": ["codegen"] } } }
JSON
lat "$BADREF" run build --dry-run ; t_bad "a task dependsOn naming nothing is rejected"
t_has "the undefined task is named" "is not defined in"
t_has "the defined tasks are listed" "Defined tasks: build"

# The `^` form resolves against the same task map, so it is checked the same way.
cat > "$BADREF/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "app", "path": "app", "auto": false, "scripts": { "build": "echo app" } } ],
  "tasks": { "build": { "dependsOn": ["^compile"] } } }
JSON
lat "$BADREF" run build --dry-run ; t_bad "a caret dependsOn naming nothing is rejected"
t_has "the caret form is checked against the task map" "'compile'"

# A resolvable graph still loads, so the check is not simply rejecting everything.
cat > "$BADREF/lattice.json" <<'JSON'
{ "workspaces": [
    { "name": "lib", "path": "lib", "auto": false, "scripts": { "build": "echo lib" } },
    { "name": "app", "path": "app", "auto": false, "dependsOn": ["lib"],
      "scripts": { "build": "echo app" } } ],
  "tasks": { "build": { "dependsOn": ["^build"] } } }
JSON
lat "$BADREF" run build --dry-run ; t_ok "a resolvable dependsOn still loads"

# --- a workspace path that leaves the repo ---------------------------------
ESC="$ENVROOT/escape"; mkdir -p "$ESC/repo"; mkdir -p "$ESC/outside"
cat > "$ESC/repo/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "esc", "path": "../outside", "auto": false,
                    "scripts": { "build": "echo x > out.txt" } } ],
  "tasks": { "build": { "outputs": ["out.txt"] } } }
JSON
lat "$ESC/repo" run build --dry-run ; t_bad "a workspace path outside the repo is rejected"
t_has "the escaping path is named" "outside the repo root"

# --- prune leaves what is not a cache format -------------------------------
# `cacheDir` can point at a directory Lattice does not own outright. Prune used
# to remove every neighbour that was not the current format, which took the
# provisioned toolchains and the installed binary with it.
KEEP="$ENVROOT/prunekeep"; mkdir -p "$KEEP/app"
cat > "$KEEP/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "app", "path": "app", "auto": false,
                    "scripts": { "build": "echo hi > out.txt" } } ],
  "tasks": { "build": { "outputs": ["out.txt"] } },
  "settings": { "cacheDir": ".lattice", "maxCacheSize": "1B" } }
JSON
w "$KEEP/.lattice/toolchains/faketool/1.0.0-abcd/bin/faketool" '#!/bin/sh
'
w "$KEEP/.lattice/bin/lattice-1.0.0" 'the binary in use'
w "$KEEP/.lattice/v1/dead.meta.json" '{}'
lat "$KEEP" run build  ; t_ok "run with cacheDir=.lattice exits 0"
lat "$KEEP" prune      ; t_ok "prune with cacheDir=.lattice exits 0"
t_file   "$KEEP/.lattice/toolchains/faketool/1.0.0-abcd/bin/faketool" "prune keeps the provisioned toolchains"
t_file   "$KEEP/.lattice/bin/lattice-1.0.0" "prune keeps the installed binary"
# Prune removes no directories at all, which is what keeps it from ever calling
# remove_dir_all on a path the user chose as cacheDir. A leftover v1/ from an
# older build is unreachable, and deleting it is the user's call, not prune's.
t_dir    "$KEEP/.lattice/v1" "prune removes no directories, including an unreachable old format"

# --- settings.maxCacheSize is enforced by the run --------------------------
# The setting reads as a budget, so it has to be one: leaving enforcement to
# `lattice prune` alone meant a repo that set one still grew without limit.
BUDGET="$ENVROOT/budget"; mkdir -p "$BUDGET/app"
cat > "$BUDGET/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "app", "path": "app", "auto": false,
                    "scripts": { "build": "head -c 200000 /dev/zero | tr '\\0' 'x' > big.bin" } } ],
  "tasks": { "build": { "outputs": ["big.bin"] } },
  "settings": { "maxCacheSize": "1KB" } }
JSON
for seed in 1 2 3; do
  w "$BUDGET/app/seed.txt" "seed$seed"
  lat "$BUDGET" run build >/dev/null 2>&1
done
BUDGET_BYTES=0
for f in "$BUDGET"/.lattice/cache/*/*.tar.gz "$BUDGET"/.lattice/cache/*/*.meta.json; do
  [ -f "$f" ] || continue
  BUDGET_BYTES=$((BUDGET_BYTES + $(wc -c < "$f")))
done
if [ "$BUDGET_BYTES" -le 1024 ]; then
  pass "a run holds the cache to settings.maxCacheSize without calling prune"
else
  fail "a run holds the cache to settings.maxCacheSize without calling prune" \
       "cache is $BUDGET_BYTES bytes against a 1KB budget"
fi

# --- a task timeout --------------------------------------------------------
# A task with no limit that never exits hangs the run, in CI as much as locally.
TMO="$ENVROOT/timeout"; mkdir -p "$TMO/app"
cat > "$TMO/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "app", "path": "app", "auto": false,
                    "scripts": { "build": "sleep 60; touch finished.txt" } } ],
  "tasks": { "build": { "timeout": "1s" } } }
JSON
lat_timeout "$TMO" 45 run build
t_ran  "an overrunning task ends the run on its own"
t_bad  "an overrunning task fails the run"
t_has  "the overrun is reported as a timeout" "timed out"
t_nofile "$TMO/app/finished.txt" "the task did not run to completion"

# A limit the task stays inside changes nothing.
cat > "$TMO/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "app", "path": "app", "auto": false,
                    "scripts": { "build": "echo hi > out.txt" } } ],
  "tasks": { "build": { "timeout": "5m", "outputs": ["out.txt"] } } }
JSON
lat "$TMO" run build ; t_ok "a task inside its timeout is untouched"
t_file "$TMO/app/out.txt" "a task inside its timeout still produces its outputs"

# A timeout on a persistent task is ignored: a dev server is asked to keep
# running, so a limit would only cut short the thing it exists to hold open.
cat > "$TMO/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "app", "path": "app", "auto": false,
                    "scripts": { "dev": "echo serving; sleep 3117" } } ],
  "tasks": { "dev": { "persistent": true, "timeout": "1s" } } }
JSON
lat_timeout "$TMO" 8 run dev
if [ "$RC" -eq 124 ]; then
  pass "a timeout on a persistent task is ignored"
else
  fail "a timeout on a persistent task is ignored" "the run ended by itself | $(snip)"
fi
pkill -f "sleep 3117" 2>/dev/null

# --- an interrupt takes the whole child tree -------------------------------
# Each task runs in its own process group, which is what lets a task that shells
# out be cleaned up as a unit — and the same call detaches it from the terminal's
# Ctrl-C. The signal reached Lattice, Lattice exited, and the children kept
# running. Lattice has to pass the signal on itself.
#
# `SIGTERM` rather than `SIGINT`: a background job in a non-interactive shell
# inherits `SIGINT` as ignored, so sending it here would prove nothing about
# Lattice. `SIGTERM` is what a CI runner sends to cancel a job anyway, and both
# take the same path.
SIG="$ENVROOT/signals"; mkdir -p "$SIG/app"
cat > "$SIG/lattice.json" <<'JSON'
{ "workspaces": [ { "name": "app", "path": "app", "auto": false,
                    "scripts": { "build": "sleep 4271" } } ],
  "tasks": { "build": {} } }
JSON
# `exec`, so `$!` is the binary's own pid. Without it the subshell stays in the
# way and the signal never reaches Lattice at all.
( cd "$SIG" && exec "$BIN" run build >/dev/null 2>&1 ) &
SIG_PID=$!
# Wait for the child to actually be up before signalling.
k=0
while [ $k -lt 100 ] && ! pgrep -f "sleep 4271" >/dev/null 2>&1; do sleep 0.1; k=$((k + 1)); done
if pgrep -f "sleep 4271" >/dev/null 2>&1; then
  pass "the task's child process is running before the interrupt"
  kill -TERM "$SIG_PID" 2>/dev/null
  # The run has the grace period to stop its children and exit.
  k=0
  while kill -0 "$SIG_PID" 2>/dev/null && [ $k -lt 150 ]; do sleep 0.1; k=$((k + 1)); done
  if kill -0 "$SIG_PID" 2>/dev/null; then
    fail "an interrupted run exits rather than hanging" "still running after 15s"
    kill -9 "$SIG_PID" 2>/dev/null
    SIG_RC=-1
  else
    pass "an interrupted run exits rather than hanging"
    wait "$SIG_PID" 2>/dev/null; SIG_RC=$?
  fi
  if pgrep -f "sleep 4271" >/dev/null 2>&1; then
    fail "an interrupt leaves no child process behind" "the child outlived the run"
    pkill -9 -f "sleep 4271" 2>/dev/null
  else
    pass "an interrupt leaves no child process behind"
  fi
  if [ "$SIG_RC" -eq 130 ]; then
    pass "an interrupted run exits 130 rather than 1"
  else
    fail "an interrupted run exits 130 rather than 1" "exit=$SIG_RC"
  fi
else
  fail "the task's child process is running before the interrupt" "child never appeared"
  kill -9 "$SIG_PID" 2>/dev/null
fi

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
late "$NESTPATH" "$NEST" run build -v ; t_ok "passthrough cold run exits 0"
t_has "inner runner was invoked"        "turbo-stub: build complete"
t_has "downstream ran after the nested repo" "api-built"
t_file "$NEST/frontend/packages/site/dist/bundle.js" "inner runner produced its artifacts"
t_nofile "$NEST/.lattice/toolchains" "passthrough repo provisions no toolchains"

late "$NESTPATH" "$NEST" run build -v ; t_ok "passthrough warm run exits 0"
t_has "nested repo caches as one unit" "frontend:build: cache hit"
t_hasnt "a hit never invokes the inner runner" "turbo-stub"

rm -rf "$NEST/frontend/packages/ui/dist" "$NEST/frontend/packages/site/dist"
late "$NESTPATH" "$NEST" run build ; t_ok "passthrough restore run exits 0"
t_file "$NEST/frontend/packages/ui/dist/bundle.js" "hit restored the inner artifacts"

w "$NEST/frontend/packages/ui/src/index.js" "ui v2 CHANGED
"
late "$NESTPATH" "$NEST" run build -v ; t_ok "passthrough run after inner edit exits 0"
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
# LATTICE_NO_PATH keeps the installer out of the developer's shell config. Its
# idempotence guard keys on the install directory, which is a fresh temp path on
# every run, so without this each invocation appends another dead PATH entry to
# the real ~/.zshrc. The PATH-editing path is covered on its own below, under a
# throwaway HOME.
RELENV="LATTICE_RELEASE_BASE_URL=$RELBASE LATTICE_NO_PATH=1"

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

  # --- PATH editing, against a throwaway HOME ----------------------------
  # The one place the installer is allowed to touch a shell config. Pointing HOME
  # and ZDOTDIR at a temp dir keeps it off the real one while still exercising the
  # code that writes the line — and, more importantly, the guard that stops it
  # being written twice.
  PATHHOME="$ENVROOT/fakehome"
  PATHINST="$ENVROOT/path-install"
  mkdir -p "$PATHHOME" "$PATHINST"
  cat > "$PATHINST/lattice.json" <<JSON
{ "latticeVersion": "$FAKEVER", "workspaces": [], "tasks": { "build": {} } }
JSON
  PATHENV="LATTICE_RELEASE_BASE_URL=$RELBASE HOME=$PATHHOME ZDOTDIR=$PATHHOME SHELL=/bin/zsh"
  OUTPUT="$(cd "$PATHINST" && env $PATHENV sh "$INSTALLER" 2>&1)"; RC=$?
  t_ok  "installer exits 0 when it may edit PATH"
  t_has "installer says where it added the line" "added .lattice/bin to PATH in"
  t_grepfile "$PATHHOME/.zshrc" "$PATHINST/.lattice/bin" "the PATH line names the install dir"

  # Second run, same install dir: the line is already there and must not repeat.
  OUTPUT="$(cd "$PATHINST" && env $PATHENV sh "$INSTALLER" 2>&1)"; RC=$?
  t_ok "installer re-run with PATH already edited exits 0"
  PATHLINES="$(grep -cF "$PATHINST/.lattice/bin" "$PATHHOME/.zshrc" 2>/dev/null || echo 0)"
  if [ "$PATHLINES" -eq 1 ]; then
    pass "a second install does not duplicate the PATH line"
  else
    fail "a second install does not duplicate the PATH line" \
      "found $PATHLINES copies in $PATHHOME/.zshrc"
  fi

  # And --no-modify-path writes nothing, it only tells you what it would have done.
  NOMOD="$ENVROOT/path-nomod"; mkdir -p "$NOMOD"
  NOMODHOME="$ENVROOT/fakehome-nomod"; mkdir -p "$NOMODHOME"
  cat > "$NOMOD/lattice.json" <<JSON
{ "latticeVersion": "$FAKEVER", "workspaces": [], "tasks": { "build": {} } }
JSON
  OUTPUT="$(cd "$NOMOD" && env "LATTICE_RELEASE_BASE_URL=$RELBASE" "HOME=$NOMODHOME" \
    "ZDOTDIR=$NOMODHOME" "SHELL=/bin/zsh" sh "$INSTALLER" --no-modify-path 2>&1)"; RC=$?
  t_ok     "installer exits 0 with --no-modify-path"
  t_has    "--no-modify-path prints the line instead of writing it" "to put it on PATH:"
  t_nofile "$NOMODHOME/.zshrc" "--no-modify-path leaves the shell config alone"
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
    LATTICE_NO_PATH=1 sh "$INSTALLER" 2>&1)"; RC=$?
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
# skill documents is imaginary. `--loquacious` is the single exception — a hidden
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
  case "$f" in --loquacious | --help | --version) continue ;; esac
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
# Every build-tool row of the skill's driver table, verified from its own
# fingerprint: these invoke a task directly, with no manifest to consult.
#
# Column positions track the table's header at
# skills/lattice/references/toolchains.md: Tool, Language, Roles, Fingerprint,
# Version command, Invoke template. A row can hold several roles, so match
# inside $4 rather than comparing it.
DRVDIR="$ENVROOT/skill-drivers"
mkdir -p "$DRVDIR"
awk -F'|' 'NF >= 7 && $4 ~ /Build tool/ {
  tool = $2; fps = $5; inv = $7
  gsub(/[` ]/, "", tool)
  gsub(/`/, "", fps); split(fps, a, ","); fp = a[1]; gsub(/^ +| +$/, "", fp)
  gsub(/`/, "", inv); gsub(/^ +| +$/, "", inv)
  print tool "\t" fp "\t" inv
}' "$SKILLTOOLS" > "$DRVDIR/rows.tsv"

# An extraction that matches nothing would build a config with no workspaces, and
# every assertion below it would pass without running. Reshaping the table in the
# skill has broken this parse before, so check the row count first.
DRVROWS="$(wc -l < "$DRVDIR/rows.tsv" | tr -d ' ')"
if [ "$DRVROWS" -eq 8 ]; then
  pass "the skill's driver table yields 8 build-tool rows"
else
  fail "the skill's driver table yields 8 build-tool rows" \
    "got $DRVROWS — check the column order against the table's header"
fi

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
# The desktop app's wiring.
#
# The app itself needs a webview and a browser, neither of which belongs in a
# hermetic suite. What is checkable here is the wiring around it: that its crate is
# in the workspace, that its config says what the build depends on it saying, and
# that it grants the webview no filesystem reach.
# =========================================================================
sect "the desktop app's wiring"

DESKTOP="$REPO_ROOT/apps/desktop"
CONF="$DESKTOP/src-tauri/tauri.conf.json"

# A crate missing from the workspace only fails in the job that builds it, which is
# not the job most changes run.
META="$(cd "$REPO_ROOT" && cargo metadata --no-deps --format-version 1 2>/dev/null)"
for crate in lattice-events lattice-project lattice-desktop; do
  if printf '%s' "$META" | grep -q "\"name\":\"$crate\""; then
    pass "cargo metadata lists $crate"
  else
    fail "cargo metadata lists $crate" "not a workspace member"
  fi
done

t_file "$CONF" "the desktop app has a tauri.conf.json"

# The version is deliberately absent so it falls back to Cargo.toml, which is why
# check-versions.sh has nothing to assert about it. A version key here would drift.
if grep -qE '^[[:space:]]*"version"[[:space:]]*:' "$CONF" 2>/dev/null; then
  fail "tauri.conf.json declares no version of its own" "found a version key; it must fall back to Cargo.toml"
else
  pass "tauri.conf.json declares no version of its own"
fi

# Tauri writes publisher, copyright and the descriptions into the MSI's main.wxs
# without escaping them, so a bare `&` in any of them is invalid XML and WiX's
# candle.exe rejects the file. It reports only `failed to run candle.exe`, with
# candle's own message swallowed, so the cause is invisible at the point it bites.
# "Lattice & Company" as the publisher is what broke the first 1.0.0 release build.
if grep -nE '"(publisher|copyright|shortDescription|longDescription|productName)"[[:space:]]*:[^,]*&' "$CONF" >/dev/null 2>&1; then
  fail "no bare & reaches the MSI's main.wxs" "a WXS-bound field in tauri.conf.json holds an unescaped &"
else
  pass "no bare & reaches the MSI's main.wxs"
fi

# devUrl and the dev server have to agree on a port, and frontendDist is what
# generate_context! embeds.
t_grepfile "$CONF" '"devUrl": "http://localhost:1420"' "tauri.conf.json points at the dev server port vite pins"
t_grepfile "$CONF" '"frontendDist": "../dist"' "tauri.conf.json points at the frontend bundle"
t_grepfile "$DESKTOP/vite.config.ts" 'port: 1420' "vite serves the port tauri.conf.json expects"
t_grepfile "$DESKTOP/vite.config.ts" 'strictPort: true' "vite refuses another port rather than leaving the window pointed at nothing"

CAPS="$DESKTOP/src-tauri/capabilities/default.json"
t_grepfile "$CAPS" 'core:default' "the desktop app grants the core defaults"

# A security invariant, not a preference: every filesystem access goes through a Rust
# command, so the webview is never handed one of its own.
if grep -q '"fs:' "$CAPS" 2>/dev/null; then
  fail "the webview is granted no filesystem permission" "found an fs: permission in $CAPS"
else
  pass "the webview is granted no filesystem permission"
fi

# The brand token files are copies, so something has to notice when they diverge. The
# script also pins the one setting they are allowed to disagree about, $primary.
if (cd "$REPO_ROOT" && node scripts/checkBrandTokens.mjs >/dev/null 2>&1); then
  pass "the desktop app's brand tokens match the website's"
else
  fail "the desktop app's brand tokens match the website's" "scripts/checkBrandTokens.mjs reported drift"
fi

# Crimson means two things in the app and no more: the product word, and failure. A
# primary action in crimson would make it mean a third.
t_grepfile "$DESKTOP/src/components/appShell.tsx" 'wordmark__product">desktop<' "the app's lockup carries the desktop product word"
t_grepfile "$DESKTOP/src/globals.scss" '.wordmark__product' "the product word has a colour rule"
t_nogrepfile "$DESKTOP/src/globals.scss" 'btn-contrast' "the app's primary action is btn-primary, not the site's ink CTA"

# Bootstrap draws a spinner from currentColor, so without a rule of its own each one
# takes the colour of whatever it sits in. Waiting is one state and reads as one colour.
t_grepfile "$DESKTOP/src/globals.scss" '.spinner-border' "every spinner takes the accent rather than its surroundings"

# Swapping projects is the thing a window that shows one project at a time is most often
# asked to do, so it lives on the project itself rather than behind an icon in a corner.
t_grepfile "$DESKTOP/src/components/appShell.tsx" '<ProjectSwitcher />' "the rail's project block is the switcher"

# The ecosystem a driver belongs to is chosen in Rust; the artwork for it lives in the
# app. A new driver in a new ecosystem would otherwise ship an empty square, and only
# somebody opening that kind of repo would ever see it.
ART="$DESKTOP/src/assets/languages"
MISSING_ART=""
for lang in $(grep -o 'language: Some("[a-z]*")' "$REPO_ROOT/crates/lattice-workspace/src/lib.rs" | sed 's/.*Some("//; s/").*//' | sort -u); do
  [ -f "$ART/$lang.svg" ] || MISSING_ART="$MISSING_ART $lang"
done
if [ -z "$MISSING_ART" ]; then
  pass "every ecosystem a driver reports has artwork in the desktop app"
else
  fail "every ecosystem a driver reports has artwork in the desktop app" "no svg for:$MISSING_ART"
fi

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
