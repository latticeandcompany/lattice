#!/usr/bin/env bash
# Write a new version into every file that states one by hand, then verify.
#
# Cargo.toml [workspace.package] is the source of truth for both the version
# (which this script overwrites) and the MSRV (which it only propagates — bump
# rust-version by hand, then run this to spread it). The crates inherit
# `version.workspace = true` and the binary reads CARGO_PKG_VERSION, so the only
# hand-written copies are the ones listed below.
#
# Usage: scripts/sync-version.sh <version>
# Example: scripts/sync-version.sh 0.2.0
#
# Finishes by running check-versions.sh, which is the same gate release.yml uses.
set -euo pipefail

if [ -z "${1-}" ]; then
	echo "Usage: $0 <version>"
	echo "Example: $0 0.2.0"
	exit 1
fi

version="$1"

if ! echo "$version" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
	echo "Error: version must be in semver format (e.g. 0.2.0)"
	exit 1
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

updated=0

note() {
	echo "  $1  $2"
	updated=$((updated + 1))
}

# .gitattributes checks some files out CRLF, so every read strips the carriage
# return first: a trailing \r turns an exact match into a silent mismatch. The
# writes below only ever substitute inside a line, so they leave it in place.
text() { tr -d '\r' <"$1"; }

# Replace a file with stdin, keeping its mode and inode. Returns 1 when stdin is
# byte-identical, so callers can report only real changes.
apply() {
	local file="$1" tmp
	tmp="$(mktemp)"
	cat >"$tmp"

	if cmp -s "$tmp" "$file"; then
		rm -f "$tmp"
		return 1
	fi

	cat "$tmp" >"$file"
	rm -f "$tmp"
}

# Only the [workspace.package] block, so a dependency's `version = ` cannot
# match. -F'"' puts the quoted value in $2.
toml_field() {
	text Cargo.toml | awk -F'"' -v key="$1" '
		/^\[workspace\.package\]/ { inblock = 1; next }
		/^\[/ { inblock = 0 }
		inblock && $0 ~ "^" key "[[:space:]]*=" { print $2; exit }'
}

json_field() {
	text "$1" | sed -n "s/.*\"$2\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" | head -n1
}

# Replace the first $3 occurrences of a "key": "value" pair, leaving deeper
# copies of the same key (a dependency named "version", a nested package entry)
# alone.
set_json_field() {
	local file="$1" key="$2" limit="${3:-1}"
	awk -v key="$key" -v value="$version" -v limit="$limit" '
		seen < limit && $0 ~ "\"" key "\"[[:space:]]*:[[:space:]]*\"" {
			sub("\"" key "\"[[:space:]]*:[[:space:]]*\"[^\"]*\"", "\"" key "\": \"" value "\"")
			seen++
		}
		{ print }
	' "$file" | apply "$file"
}

# --- read the source of truth ------------------------------------------------
current="$(toml_field version)"
msrv="$(toml_field rust-version)"

[ -n "$current" ] || { echo 'no version in [workspace.package]' >&2; exit 2; }
[ -n "$msrv" ] || { echo 'no rust-version in [workspace.package]' >&2; exit 2; }

echo "Cargo.toml: version $current ~> $version, rust-version $msrv (unchanged)"
echo ""

# --- 1. the version ----------------------------------------------------------
# The `next` on the block header keeps the following /^\[/ rule from closing the
# block it just opened, and `seen` stops a later [dependencies] version = line
# from being rewritten too.
if awk -v value="$version" '
	/^\[workspace\.package\]/ { inblock = 1; print; next }
	/^\[/ { inblock = 0 }
	inblock && !seen && /^version[[:space:]]*=/ { sub(/"[^"]*"/, "\"" value "\""); seen = 1 }
	{ print }
' Cargo.toml | apply Cargo.toml; then
	note "Cargo.toml" "[workspace.package] version = $version"
fi

if set_json_field lattice.json latticeVersion; then
	note "lattice.json" "latticeVersion = $version"
fi

if set_json_field apps/web/package.json version; then
	note "apps/web/package.json" "version = $version"
fi

# The lockfile repeats the root version twice — top level and packages."" — and
# `npm ci` fails on a mismatch with package.json.
if set_json_field apps/web/package-lock.json version 2; then
	note "apps/web/package-lock.json" "version = $version (both root entries)"
fi

# `lattice init` writes CARGO_PKG_VERSION into new configs, so the example
# configs should read as if this release generated them. Their apps' own
# package.json versions are the examples' versions, not ours, and stay put.
while IFS= read -r file; do
	if set_json_field "$file" latticeVersion; then
		note "$file" "latticeVersion = $version"
	fi
done < <(find examples -name lattice.json -not -path "*/node_modules/*")

# The config sample in the README is the first lattice.json most people read.
if sed "s|\(\"latticeVersion\"[[:space:]]*:[[:space:]]*\"\)[^\"]*\"|\1$version\"|" \
	.github/README.md | apply .github/README.md; then
	note ".github/README.md" "sample latticeVersion = $version"
fi

# Nothing writes a version badge: check-versions.sh asserts the hardcoded one
# stays gone, because the github/v/release shield reads the real release.

# --- 2. the MSRV, propagated from rust-version -------------------------------
# check-versions.sh flags stale prose copies (it still greps for an old 1.75),
# so sync the shield, the engines floor, and the sentences that quote it.
if sed "s|\(badge/Rust-\)[^-]*\(-000000\)|\1${msrv}+\2|" \
	.github/README.md | apply .github/README.md; then
	note ".github/README.md" "Rust badge = $msrv"
fi

engines="$(json_field lattice.json cargo)"
if [ "$engines" != ">=$msrv.0" ]; then
	if sed "s|\(\"cargo\"[[:space:]]*:[[:space:]]*\"\)[^\"]*\"|\1>=$msrv.0\"|" \
		lattice.json | apply lattice.json; then
		note "lattice.json" "engines.cargo = >=$msrv.0"
	fi
fi

for file in .github/README.md .github/CONTRIBUTING.md apps/web/src/content/docs/getting-started.md; do
	[ -f "$file" ] || continue

	# The three shapes in use: "Rust 1.86+", "Rust 1.86 or newer", and
	# CONTRIBUTING's "Rust stable (1.86+)".
	if sed -E "s|Rust stable \([0-9]+\.[0-9]+\+\)|Rust stable (${msrv}+)|g; \
		s|Rust [0-9]+\.[0-9]+\+|Rust ${msrv}+|g; \
		s|Rust [0-9]+\.[0-9]+ or newer|Rust ${msrv} or newer|g" \
		"$file" | apply "$file"; then
		note "$file" "prose MSRV = $msrv"
	fi
done

# --- 3. Cargo.lock -----------------------------------------------------------
# A forgotten `cargo update -w` after a bump makes --locked release builds fail.
# Only the seven workspace members changed, so the offline resolve is enough.
echo ""
if command -v cargo >/dev/null 2>&1; then
	if cargo update --workspace --offline --quiet 2>/dev/null; then
		echo "  Cargo.lock  workspace crates resolved to $version"
	elif cargo update --workspace --quiet; then
		echo "  Cargo.lock  workspace crates resolved to $version"
	else
		echo "  Cargo.lock  NOT updated — run \`cargo update -w\` yourself" >&2
	fi
else
	echo "  Cargo.lock  NOT updated — cargo is not on PATH, run \`cargo update -w\`" >&2
fi

# --- 4. verify ---------------------------------------------------------------
echo ""
echo "$updated file(s) updated to $version"
echo ""
exec "$root/scripts/check-versions.sh" "$version"
