#!/bin/sh
# Lattice installer. Run from the root of the repo you want to use it in:
#
#   curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh
#
# Everything lands in ./.lattice/bin — no global paths, no package manager. That
# directory is then added to PATH in your shell config, so `lattice` in this repo
# means the version this repo pins. To skip that edit:
#
#   curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh -s -- --no-modify-path
#
# or set LATTICE_NO_PATH=1. To remove it: rm -rf .lattice, and delete the lattice
# line from the shell config named at the end of the install.
#
# This runs anywhere there is a POSIX shell, Git Bash and WSL2 included. In
# PowerShell, use install.ps1 instead.
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
LIST_URL="${LATTICE_RELEASE_LIST_URL:-https://api.github.com/repos/$REPO/releases?per_page=20}"

BOLD=''; DIM=''; RED=''; RST=''
if [ -t 1 ]; then
	BOLD="$(printf '\033[1m')"; DIM="$(printf '\033[2m')"
	RED="$(printf '\033[31m')"; RST="$(printf '\033[0m')"
fi

say() { printf '%s\n' "$*"; }
die() { printf '%serror:%s %s\n' "$RED" "$RST" "$1" >&2; exit 1; }

# --- options -----------------------------------------------------------------
MODIFY_PATH=1
[ -z "${LATTICE_NO_PATH:-}" ] || MODIFY_PATH=0
for arg in "$@"; do
	case "$arg" in
		--no-modify-path) MODIFY_PATH=0 ;;
		*) die "unknown option: $arg" ;;
	esac
done

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

# Empty everywhere but Windows, where it is part of the binary's name rather
# than a decoration: PATH resolution there is by extension.
EXE=''

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
	MINGW* | MSYS* | CYGWIN*)
		# Git Bash, MSYS2 and Cygwin are POSIX shells over a native Windows
		# filesystem, so what belongs here is the Windows binary -- not the Linux
		# one, which is what installing under WSL2 gets you.
		EXE='.exe'
		case "$arch" in
			x86_64) TARGET='x86_64-pc-windows-msvc' ;;
			aarch64 | arm64)
				# No aarch64-pc-windows-msvc build is published yet. Windows on ARM
				# runs x64 binaries under emulation, so this works -- say so rather
				# than let it look like a native build.
				TARGET='x86_64-pc-windows-msvc'
				say "${DIM}no native arm64 build yet — installing the x64 build, which Windows runs under emulation${RST}"
				;;
			*) die "unsupported Windows architecture: $arch" ;;
		esac
		;;
	Windows_NT)
		die 'this installer needs a POSIX shell. Run it from Git Bash or WSL2, or
       use install.ps1 in PowerShell:

         irm https://latticeandcompany.github.io/lattice/install.ps1 | iex'
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

# The first tag_name in a GitHub releases payload, or empty. Commas become
# newlines first: the API answers on a single line, and sed's leading `.*` is
# greedy, so on a list response it would otherwise keep the oldest tag on that
# line rather than the newest.
first_tag() {
	tr ',' '\n' |
		sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"v\{0,1\}\([^"]*\)".*/\1/p' |
		head -n1
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
	# /releases/latest is the last *stable* release, so it 404s for a project whose
	# every release so far is a pre-release. That 404 is an expected answer here
	# rather than a failure, hence the discarded stderr; the fallback keeps its own,
	# because by then there is nothing left to try. The full list is ordered
	# newest-first and, unauthenticated, contains no drafts.
	VERSION="$(fetch_text "$LATEST_URL" 2>/dev/null | first_tag)"
	if [ -z "$VERSION" ]; then
		VERSION="$(fetch_text "$LIST_URL" | first_tag)"
	fi
	[ -n "$VERSION" ] || die "could not find a release to install
       tried $LATEST_URL
         and $LIST_URL"
	SOURCE='newest release'
	# Read off the version itself rather than off which URL answered, so this says
	# nothing the downloaded artifact does not.
	case "$VERSION" in
		*-*) say "${DIM}$VERSION is a pre-release — no stable release yet${RST}" ;;
	esac
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
VERSIONED="$BIN_DIR/lattice-$VERSION$EXE"
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
	extracted="$(find "$TMP" -type f -name "lattice$EXE" | head -n1)"
	[ -n "$extracted" ] || die "$ASSET contains no lattice$EXE binary"

	# Replace rather than write in place: overwriting a running binary is what
	# "Text file busy" is, and on macOS it invalidates a cached code signature.
	rm -f "$VERSIONED"
	mv "$extracted" "$VERSIONED"
	chmod +x "$VERSIONED"
fi

# --- link --------------------------------------------------------------------
if [ -n "$EXE" ]; then
	# Symlinks need a privilege Windows does not grant by default, so the stable
	# path is a copy there -- the same thing `lattice upgrade` does. A running
	# .exe cannot be overwritten but can be renamed out of the way, which is what
	# makes replacing the binary you are currently running work at all.
	PID=$$
	# A previous run leaves one behind whenever the binary it was replacing was
	# still running, which is the case that made the rename necessary.
	rm -f "$BIN_DIR"/.lattice.old-*"$EXE" 2>/dev/null || true
	if [ -e "$BIN_DIR/lattice$EXE" ]; then
		evicted="$BIN_DIR/.lattice.old-$PID$EXE"
		mv -f "$BIN_DIR/lattice$EXE" "$evicted" 2>/dev/null ||
			die "could not replace $BIN_DIR/lattice$EXE; close any running lattice and try again"
		rm -f "$evicted" 2>/dev/null || true
	fi
	cp -f "$VERSIONED" "$BIN_DIR/lattice$EXE"
else
	# Relative target, swapped through a rename, so the repo stays movable and no
	# invocation can catch .lattice/bin/lattice missing.
	ln -sfn "lattice-$VERSION" "$BIN_DIR/.lattice.link-tmp-$$"
	mv -f "$BIN_DIR/.lattice.link-tmp-$$" "$BIN_DIR/lattice"
fi

# Machine-local and ephemeral; never a commit.
if [ -f .gitignore ] && ! grep -q '^\.lattice/bin/$' .gitignore; then
	printf '.lattice/bin/\n' >>.gitignore
	say "${DIM}added .lattice/bin/ to .gitignore${RST}"
fi

# --- PATH --------------------------------------------------------------------
# Absolute, because a line in a shell config cannot be relative to whatever
# directory you happen to be standing in when the shell starts.
BIN_ABS="$PWD/$BIN_DIR"

# The line is written single-quoted, which a path containing a single quote would
# break. Rare enough to hand back rather than escape around.
case "$BIN_ABS" in
	*\'*) MODIFY_PATH=0 ;;
esac

# Which files the user's shell will actually read.
rc_files() {
	case "${SHELL##*/}" in
		zsh) printf '%s\n' "${ZDOTDIR:-$HOME}/.zshrc" ;;
		bash)
			printf '%s\n' "$HOME/.bashrc"
			# A macOS Terminal tab is a login shell: it reads .bash_profile and
			# never .bashrc unless .bash_profile says to.
			if [ -f "$HOME/.bash_profile" ] && ! grep -q 'bashrc' "$HOME/.bash_profile"; then
				printf '%s\n' "$HOME/.bash_profile"
			fi
			;;
		fish) printf '%s\n' "${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish" ;;
		*) printf '%s\n' "$HOME/.profile" ;;
	esac
}

# fish keeps PATH in its own list and has its own syntax for prepending to it.
path_line() {
	case "$1" in
		fish | *.fish) printf "fish_add_path '%s'\n" "$BIN_ABS" ;;
		*) printf "export PATH='%s':\"\$PATH\"\n" "$BIN_ABS" ;;
	esac
}

ON_PATH=0
case ":$PATH:" in
	*":$BIN_ABS:"*) ON_PATH=1 ;;
esac

if [ "$ON_PATH" -eq 1 ]; then
	PATH_NOTE="${DIM}already on PATH${RST}"
elif [ "$MODIFY_PATH" -eq 0 ]; then
	PATH_NOTE="  to put it on PATH:  $(path_line "${SHELL##*/}")"
else
	# Newline-split the list so a $HOME with a space in it survives.
	RC_LIST="$(rc_files)"
	RC_IFS="$IFS"
	IFS='
'
	set -f
	EDITED=''
	FOUND=''
	for rc in $RC_LIST; do
		if [ -f "$rc" ] && grep -qF "$BIN_ABS" "$rc"; then
			FOUND="$FOUND $rc"
			continue
		fi
		mkdir -p "$(dirname "$rc")" 2>/dev/null || true
		# Pick the syntax before the redirection, not inside it: naming $rc within
		# a block that appends to $rc reads as writing a file being read (SC2094).
		RC_LINE="$(path_line "$rc")"
		# The subshell is what makes 2>/dev/null cover a failed redirection too:
		# that error is reported by the shell opening the file, not by printf.
		if ( { printf '\n# lattice (%s)\n' "$PWD"; printf '%s\n' "$RC_LINE"; } >>"$rc" ) 2>/dev/null; then
			EDITED="$EDITED $rc"
		else
			say "${DIM}could not write $rc${RST}"
		fi
	done
	set +f
	IFS="$RC_IFS"

	if [ -n "$EDITED" ]; then
		PATH_NOTE="${DIM}added .lattice/bin to PATH in${RST}${EDITED} ${DIM}— open a new shell to use it${RST}"
		# That line is read by this shell and no other. PowerShell and cmd resolve
		# PATH from the environment, which install.ps1 is what edits.
		[ -z "$EXE" ] ||
			PATH_NOTE="$PATH_NOTE
${DIM}  for PowerShell and cmd, install.ps1 sets the user PATH instead${RST}"
	elif [ -n "$FOUND" ]; then
		PATH_NOTE="${DIM}already in${RST}${FOUND} ${DIM}— open a new shell to use it${RST}"
	else
		PATH_NOTE="  to put it on PATH:  $(path_line "${SHELL##*/}")"
	fi
fi

# Only a shell that has already picked the line up can run the bare name.
if [ "$ON_PATH" -eq 1 ]; then
	RUN='lattice'
else
	RUN="./.lattice/bin/lattice$EXE"
fi

say ''
say "✓ ${BOLD}lattice $VERSION${RST} installed at $VERSIONED"
say "$PATH_NOTE"
say ''
say "  run       ${BOLD}$RUN run build${RST}"
say "  commands  $RUN --help"
say "  remove    rm -rf .lattice"
if [ ! -f lattice.json ]; then
	say ''
	say "  This directory has no lattice.json yet. $RUN init writes one."
fi
