#!/bin/sh
# Assert the version and MSRV are consistent everywhere they are written by hand.
#
# Cargo.toml [workspace.package] is the source of truth. The binary derives its
# version from CARGO_PKG_VERSION and `lattice init` writes that into new configs,
# so those cannot drift. These can:
#
#   lattice.json           latticeVersion   (this repo dogfooding itself)
#   apps/web/package.json  version
#   lattice.json           engines.cargo    (must match rust-version)
#   .github/README.md      the Rust badge   (must match rust-version)
#
# Usage: scripts/check-versions.sh [expected-version]
# With an argument, every version must also equal it — that is how release.yml
# refuses to build a tag that disagrees with the tree.
set -eu

ROOT="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$ROOT"

RED=''; GRN=''; RST=''
if [ -t 1 ]; then RED="$(printf '\033[31m')"; GRN="$(printf '\033[32m')"; RST="$(printf '\033[0m')"; fi

fails=0
bad() { printf '%s✗%s %s\n' "$RED" "$RST" "$1"; fails=$((fails + 1)); }
good() { printf '%s✓%s %s\n' "$GRN" "$RST" "$1"; }

# --- read the source of truth ------------------------------------------------
# .gitattributes checks these files out CRLF, so every read strips the carriage
# return first: a trailing \r turns an exact match into a silent mismatch and
# leaves an extracted value looking empty.
text() { tr -d '\r' <"$1"; }

# Only the [workspace.package] block, so a dependency's `version = ` cannot match.
# `-F'"'` puts the quoted value in $2; a `gsub` of everything up to a quote is
# greedy enough to swallow the value along with the key.
toml_field() {
	text Cargo.toml | awk -F'"' -v key="$1" '
		/^\[workspace\.package\]/ { inblock = 1; next }
		/^\[/ { inblock = 0 }
		inblock && $0 ~ "^" key "[[:space:]]*=" { print $2; exit }'
}

CARGO_VERSION="$(toml_field version)"
MSRV="$(toml_field rust-version)"

[ -n "$CARGO_VERSION" ] || { printf 'no version in [workspace.package]\n' >&2; exit 2; }
[ -n "$MSRV" ] || { printf 'no rust-version in [workspace.package]\n' >&2; exit 2; }

printf 'Cargo.toml: version %s, rust-version %s\n\n' "$CARGO_VERSION" "$MSRV"

json_field() {
	text "$1" | sed -n "s/.*\"$2\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" | head -n1
}

# --- 1. versions agree -------------------------------------------------------
v="$(json_field lattice.json latticeVersion)"
if [ "$v" = "$CARGO_VERSION" ]; then good "lattice.json latticeVersion = $v"
else bad "lattice.json latticeVersion is $v, want $CARGO_VERSION"; fi

v="$(json_field apps/web/package.json version)"
if [ "$v" = "$CARGO_VERSION" ]; then good "apps/web/package.json version = $v"
else bad "apps/web/package.json version is $v, want $CARGO_VERSION"; fi

# --- 2. Cargo.lock agrees for every workspace crate --------------------------
# A forgotten `cargo update -w` after a bump makes --locked release builds fail.
missing=''
for crate in lattice lattice-cache lattice-config dagger lattice-output lattice-runner lattice-workspace; do
	text Cargo.lock | grep -q "^name = \"$crate\"\$" ||
		{ missing="$missing $crate(absent)"; continue; }
	text Cargo.lock | awk -F'"' -v c="$crate" -v want="$CARGO_VERSION" '
		$0 == "name = \"" c "\"" { getline; if ($2 != want) exit 1; exit 0 }
	' || missing="$missing $crate"
done
if [ -z "$missing" ]; then good "Cargo.lock agrees for all 7 crates"
else bad "Cargo.lock disagrees for:$missing — run \`cargo update -w\`"; fi

# --- 3. the hardcoded version badge stays dead -------------------------------
# It was replaced by a shield that reads the release from GitHub. If it comes
# back it will be silently wrong within a release.
if grep -q 'img\.shields\.io/badge/version-' .github/README.md; then
	bad 'README has a hardcoded version badge; use the github/v/release shield'
else
	good 'README has no hardcoded version badge'
fi

# --- 4. the MSRV is stated consistently --------------------------------------
v="$(json_field lattice.json cargo)"
if [ "$v" = ">=$MSRV.0" ]; then good "lattice.json engines.cargo = $v"
else bad "lattice.json engines.cargo is $v, want >=$MSRV.0"; fi

if grep -q "Rust-$MSRV+-" .github/README.md; then good "README Rust badge = $MSRV"
else bad "README Rust badge does not say $MSRV"; fi

for f in .github/README.md .github/CONTRIBUTING.md apps/web/src/content/docs/getting-started.md; do
	if grep -q "1\.75" "$f" 2>/dev/null; then bad "$f still claims Rust 1.75"; fi
done

# --- 5. optional pin against a tag -------------------------------------------
if [ "$#" -ge 1 ]; then
	if [ "$CARGO_VERSION" = "$1" ]; then good "matches expected version $1"
	else bad "tree says $CARGO_VERSION, expected $1"; fi
fi

printf '\n'
if [ "$fails" -ne 0 ]; then
	printf '%s%s check(s) failed%s\n' "$RED" "$fails" "$RST"
	exit 1
fi
printf '%sall version checks passed%s\n' "$GRN" "$RST"
