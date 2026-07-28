#!/bin/sh
# Lattice installer. Run from the root of the repo you want to use it in:
#
#   curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh
#
# Everything lands in ./.lattice/bin — no global paths, no package manager, no
# PATH edits. To remove it: rm -rf .lattice
#
# Which version it installs, in order:
#   1. $LATTICE_VERSION, if set
#   2. latticeVersion from ./lattice.json — the committed config is the lockfile,
#      which is why this is read here rather than by a binary that does not exist yet
#   3. the newest release, when the directory has no lattice.json at all
#
# A lattice.json that exists but pins nothing is an error, not a reason to guess.
set -eu

REPO="latticeandcompany/lattice"
BASE_URL="${LATTICE_RELEASE_BASE_URL:-https://github.com/$REPO/releases/download}"
LATEST_URL="${LATTICE_RELEASE_LATEST_URL:-https://api.github.com/repos/$REPO/releases/latest}"

BOLD=''; DIM=''; RED=''; RST=''
if [ -t 1 ]; then
	BOLD="$(printf '\033[1m')"; DIM="$(printf '\033[2m')"
	RED="$(printf '\033[31m')"; RST="$(printf '\033[0m')"
fi

say() { printf '%s\n' "$*"; }
die() { printf '%serror:%s %s\n' "$RED" "$RST" "$1" >&2; exit 1; }

# --- fetchers ----------------------------------------------------------------
if command -v curl >/dev/null 2>&1; then
	fetch_file() { curl -fsSL --retry 2 -o "$2" "$1"; }
	fetch_text() { curl -fsSL --retry 2 "$1"; }
elif command -v wget >/dev/null 2>&1; then
	fetch_file() { wget -q -O "$2" "$1"; }
	fetch_text() { wget -q -O- "$1"; }
else
	die 'neither curl nor wget is installed'
fi

# --- target ------------------------------------------------------------------
os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
	Darwin)
		case "$arch" in
			arm64 | aarch64) TARGET='aarch64-apple-darwin' ;;
			x86_64) TARGET='x86_64-apple-darwin' ;;
			*) die "unsupported macOS architecture: $arch" ;;
		esac
		;;
	Linux)
		# A musl system has no glibc loader to report a version.
		libc='gnu'
		if ! (ldd --version 2>&1 | grep -qi 'glibc\|gnu libc'); then
			libc='musl'
		fi
		case "$arch" in
			x86_64) TARGET="x86_64-unknown-linux-$libc" ;;
			aarch64 | arm64)
				[ "$libc" = 'gnu' ] || die 'aarch64 musl is not published; build from source'
				TARGET='aarch64-unknown-linux-gnu'
				;;
			*) die "unsupported Linux architecture: $arch" ;;
		esac
		;;
	MINGW* | MSYS* | CYGWIN* | Windows_NT)
		die 'this installer needs a POSIX shell. On Windows, run it inside WSL2'
		;;
	*)
		die "unsupported operating system: $os"
		;;
esac

# --- checksum tool -----------------------------------------------------------
if command -v sha256sum >/dev/null 2>&1; then
	sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
	sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
elif command -v openssl >/dev/null 2>&1; then
	sha256() { openssl dgst -sha256 "$1" | awk '{ print $NF }'; }
else
	die 'no sha256 tool found (sha256sum, shasum, or openssl); cannot verify the download'
fi

# --- version -----------------------------------------------------------------
read_pin() {
	# The pin, or empty. Deliberately narrow: only a top-level string value.
	sed -n 's/.*"latticeVersion"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' lattice.json | head -n1
}

if [ -n "${LATTICE_VERSION:-}" ]; then
	VERSION="${LATTICE_VERSION#v}"
	# A label to print, not a reference — the literal variable name is the point.
	# shellcheck disable=SC2016
	SOURCE='$LATTICE_VERSION'
elif [ -f lattice.json ]; then
	VERSION="$(read_pin)"
	[ -n "$VERSION" ] || die 'lattice.json has no "latticeVersion". Add the version this repo
       should use, or set LATTICE_VERSION to install a specific one.'
	VERSION="${VERSION#v}"
	SOURCE='lattice.json'
else
	say "${DIM}no lattice.json here — installing the newest release${RST}"
	VERSION="$(fetch_text "$LATEST_URL" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"v\{0,1\}\([^"]*\)".*/\1/p' | head -n1)"
	[ -n "$VERSION" ] || die "could not read the newest release from $LATEST_URL"
	SOURCE='newest release'
fi

# A version reaches a URL and a filename, so anything but a version stops here.
case "$VERSION" in
	[0-9]*) ;;
	*) die "'$VERSION' does not look like a version" ;;
esac
case "$VERSION" in
	*[!0-9.a-zA-Z+-]*) die "'$VERSION' does not look like a version" ;;
esac

BIN_DIR='.lattice/bin'
VERSIONED="$BIN_DIR/lattice-$VERSION"
ASSET="lattice-$VERSION-$TARGET.tar.gz"
CHECKSUMS="lattice-$VERSION-checksums.txt"

say "${BOLD}lattice${RST} $VERSION ${DIM}($TARGET, from $SOURCE)${RST}"

mkdir -p "$BIN_DIR"

# --- download, verify, install ----------------------------------------------
if [ -x "$VERSIONED" ]; then
	say "${DIM}already downloaded${RST}"
else
	TMP="$(mktemp -d "${TMPDIR:-/tmp}/lattice-install.XXXXXX")"
	trap 'rm -rf "$TMP"' EXIT INT TERM

	say "◇ downloading $ASSET"
	fetch_file "$BASE_URL/v$VERSION/$ASSET" "$TMP/$ASSET" ||
		die "no release asset for this platform: $BASE_URL/v$VERSION/$ASSET"
	fetch_file "$BASE_URL/v$VERSION/$CHECKSUMS" "$TMP/$CHECKSUMS" ||
		die "could not download $CHECKSUMS; refusing to install an unverified binary"

	expected="$(sed -n "s/^\([0-9a-fA-F]\{64\}\)[[:space:]][[:space:]]*[*]\{0,1\}.*$ASSET\$/\1/p" "$TMP/$CHECKSUMS" | head -n1)"
	[ -n "$expected" ] || die "$CHECKSUMS does not list $ASSET"
	actual="$(sha256 "$TMP/$ASSET")"
	if [ "$expected" != "$actual" ]; then
		die "checksum mismatch for $ASSET
       expected $expected
       actual   $actual"
	fi
	say "✓ checksum verified"

	tar -xzf "$TMP/$ASSET" -C "$TMP" || die "could not extract $ASSET"
	extracted="$(find "$TMP" -type f -name lattice -perm -u+x | head -n1)"
	[ -n "$extracted" ] || die "$ASSET contains no lattice binary"

	# Replace rather than write in place: overwriting a running binary is what
	# "Text file busy" is, and on macOS it invalidates a cached code signature.
	rm -f "$VERSIONED"
	mv "$extracted" "$VERSIONED"
	chmod +x "$VERSIONED"
fi

# --- link --------------------------------------------------------------------
# Relative target, swapped through a rename, so the repo stays movable and no
# invocation can catch .lattice/bin/lattice missing.
ln -sfn "lattice-$VERSION" "$BIN_DIR/.lattice.link-tmp-$$"
mv -f "$BIN_DIR/.lattice.link-tmp-$$" "$BIN_DIR/lattice"

# Machine-local and ephemeral; never a commit.
if [ -f .gitignore ] && ! grep -q '^\.lattice/bin/$' .gitignore; then
	printf '.lattice/bin/\n' >>.gitignore
	say "${DIM}added .lattice/bin/ to .gitignore${RST}"
fi

say ''
say "✓ ${BOLD}lattice $VERSION${RST} installed at $VERSIONED"
say ''
say "  run       ${BOLD}./.lattice/bin/lattice run build${RST}"
say "  commands  ./.lattice/bin/lattice --help"
say "  remove    rm -rf .lattice"
if [ ! -f lattice.json ]; then
	say ''
	say "  This directory has no lattice.json yet. ./.lattice/bin/lattice init writes one."
fi
