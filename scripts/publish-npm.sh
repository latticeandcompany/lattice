#!/usr/bin/env bash
# Publish a released version of Lattice to npm.
#
# This does not build anything Rust. It takes a release that already exists on
# GitHub, verifies its archives against the checksums published beside them, and
# repackages those exact bytes as npm packages — so the tarball npm serves and the
# tarball install.sh serves are the same binary, and neither is built twice.
#
# Seven packages go out, in an order that matters:
#
#   @latticeandcompany/lattice-<platform>   six of them, one binary each
#   @latticeandcompany/lattice              the wrapper, which depends on all six
#
# The wrapper is last because it pins its optionalDependencies to this exact
# version. Published first, it would be installable for as long as it took the rest
# to upload, and broken for everyone who tried.
#
# All seven are assembled in a temporary directory. Nothing is published out of the
# working tree, so a publish cannot modify the repo and an interrupted one leaves
# nothing behind.
#
# Usage: scripts/publish-npm.sh <version> [--dry-run]
# Example: scripts/publish-npm.sh 1.0.0-beta-3 --dry-run
#
# Needs `gh` authenticated for the download, and `npm whoami` to succeed for the
# publish. A pre-release version is published under the `next` dist-tag, so a bare
# `npm install @latticeandcompany/lattice` keeps resolving to the last stable one.
set -euo pipefail

DRY_RUN=0
VERSION=''

for arg in "$@"; do
	case "$arg" in
		--dry-run) DRY_RUN=1 ;;
		-*) echo "unknown option: $arg" >&2; exit 2 ;;
		*)
			[ -z "$VERSION" ] || { echo "give one version, not two" >&2; exit 2; }
			VERSION="${arg#v}"
			;;
	esac
done

if [ -z "$VERSION" ]; then
	echo "Usage: $0 <version> [--dry-run]"
	echo "Example: $0 1.0.0-beta-3 --dry-run"
	exit 2
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PKG="$ROOT/packages/npm"
TAG="v$VERSION"

BOLD=''; DIM=''; RED=''; GRN=''; RST=''
if [ -t 1 ]; then
	BOLD="$(printf '\033[1m')"; DIM="$(printf '\033[2m')"
	RED="$(printf '\033[31m')"; GRN="$(printf '\033[32m')"; RST="$(printf '\033[0m')"
fi
step() { printf '\n%s==>%s %s\n' "$BOLD" "$RST" "$1"; }
die() { printf '%serror:%s %s\n' "$RED" "$RST" "$1" >&2; exit 1; }

for tool in gh npm node tar; do
	command -v "$tool" >/dev/null 2>&1 || die "$tool is not on PATH"
done

# --- 1. the tree, the tag and the request must all agree ----------------------
step "Checking the tree says $VERSION"
scripts/check-versions.sh "$VERSION" >/dev/null || die 'the tree disagrees with that version — run scripts/check-versions.sh'
gh release view "$TAG" >/dev/null 2>&1 || die "no release $TAG on GitHub — publish npm only from a released tag"
printf '%s✓%s tree, tag and argument all say %s\n' "$GRN" "$RST" "$VERSION"

# --- 2. fetch the archives and check them against their checksums -------------
WORK="$(mktemp -d)"
STAGED="$WORK/packages"
EXTRACTED="$WORK/extracted"
mkdir -p "$STAGED" "$EXTRACTED"

trap 'rm -rf "$WORK"' EXIT

step "Downloading $TAG"
gh release download "$TAG" \
	--dir "$WORK" \
	--pattern "lattice-$VERSION-*.tar.gz" \
	--pattern "lattice-$VERSION-checksums.txt" \
	--clobber

CHECKSUMS="$WORK/lattice-$VERSION-checksums.txt"
[ -f "$CHECKSUMS" ] || die "the release has no lattice-$VERSION-checksums.txt"

if command -v sha256sum >/dev/null 2>&1; then
	sha256() { sha256sum "$1" | cut -d' ' -f1; }
else
	sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
fi

step 'Verifying checksums'
count=0
for archive in "$WORK"/lattice-"$VERSION"-*.tar.gz; do
	name="$(basename "$archive")"
	want="$(awk -v f="$name" '$2 == f || $2 == "*" f { print $1; exit }' "$CHECKSUMS")"
	[ -n "$want" ] || die "$name is not listed in the checksums file"
	got="$(sha256 "$archive")"
	[ "$want" = "$got" ] || die "$name does not match its published checksum"
	printf '%s✓%s %s\n' "$GRN" "$RST" "$name"
	tar -xzf "$archive" -C "$EXTRACTED"
	count=$((count + 1))
done
[ "$count" -eq 6 ] || die "expected 6 archives, found $count"

# --- 3. build the wrapper -----------------------------------------------------
step 'Building the wrapper'
(cd "$PKG" && npm ci && npm run build && npm run check && npm test)

# A CRLF shebang makes the bin unrunnable on macOS and Linux ("env: node\r"). The
# sources are checked out CRLF by .gitattributes, so this is one bundler setting
# away from shipping broken and is worth asserting rather than assuming.
if head -n1 "$PKG/dist/cli.mjs" | grep -q $'\r'; then
	die 'dist/cli.mjs has a CRLF shebang'
fi
printf '%s✓%s the bin has a clean shebang\n' "$GRN" "$RST"

# --- 4. stage the platform packages -------------------------------------------
step 'Staging the seven packages'
# Read into an array the long way: macOS ships bash 3.2, which has no `mapfile`.
# stage.mjs prints the wrapper last, and that is the order they are published in.
(cd "$PKG" && node scripts/stage.mjs "$VERSION" "$EXTRACTED" "$STAGED") >"$WORK/staged.txt"
DIRS=()
while IFS= read -r line; do DIRS+=("$line"); done <"$WORK/staged.txt"
[ "${#DIRS[@]}" -eq 7 ] || die "staged ${#DIRS[@]} packages, expected 7"

# The one binary this machine can actually run gets asked what it is. It is the only
# available proof that the archives are the version they claim to be.
HOST="$(cd "$PKG" && node scripts/host-target.mjs)"
if [ -n "$HOST" ]; then
	host_pkg="${HOST%% *}"
	host_exe="${HOST##* }"
	exe="$STAGED/$host_pkg/bin/$host_exe"
	reported="$("$exe" --version 2>/dev/null || true)"
	case "$reported" in
		*"$VERSION"*) printf '%s✓%s %s reports: %s\n' "$GRN" "$RST" "$host_pkg" "$reported" ;;
		'') die "$exe would not run" ;;
		*) die "$exe reports '$reported', not $VERSION" ;;
	esac
fi

for dir in "${DIRS[@]}"; do
	printf '  %s%s%s\n' "$DIM" "$(basename "$dir")" "$RST"
done

# --- 5. publish ---------------------------------------------------------------
# A semver pre-release goes out under `next`, matching release.yml marking the same
# versions as GitHub pre-releases: neither should become what an unqualified install
# picks up.
case "$VERSION" in
	*-*) NPM_TAG='next' ;;
	*) NPM_TAG='latest' ;;
esac

PUBLISH=(npm publish --access public --tag "$NPM_TAG")
if [ "$DRY_RUN" -eq 1 ]; then
	PUBLISH+=(--dry-run)
	step "Dry run — nothing will be published (dist-tag would be $NPM_TAG)"
else
	npm whoami >/dev/null 2>&1 || die 'not logged in to npm — run `npm login`'
	step "Publishing $VERSION to npm under the $NPM_TAG tag"
fi

for dir in "${DIRS[@]}"; do
	(cd "$dir" && "${PUBLISH[@]}")
done

printf '\n%s%s7 packages %s%s\n' "$GRN" "$BOLD" \
	"$([ "$DRY_RUN" -eq 1 ] && echo 'would be published' || echo 'published')" "$RST"
if [ "$DRY_RUN" -eq 0 ]; then
	printf '%snpx %s@%s --version%s\n' "$DIM" '@latticeandcompany/lattice' "$VERSION" "$RST"
fi
