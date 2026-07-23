#!/usr/bin/env bash
# dev-unlink.sh — bash fallback for `lattice dev-unlink` (PRD §13).
#
# Restores the stable ./.lattice/bin/lattice symlink to the pinned release binary
# (lattice-<latticeVersion>), reading the version from the repo's lattice.json.
# Non-destructive: if the pinned release binary is not installed, nothing is changed.
set -euo pipefail

# Resolve the repo root as the nearest ancestor containing lattice.json.
find_root() {
  local dir="$PWD"
  while [[ "$dir" != "/" ]]; do
    if [[ -f "$dir/lattice.json" ]]; then
      printf '%s\n' "$dir"
      return 0
    fi
    dir="$(dirname "$dir")"
  done
  return 1
}

ROOT="$(find_root)" || {
  echo "error: no lattice.json found in this directory or any parent." >&2
  exit 1
}

cd "$ROOT"

# Read "latticeVersion" from lattice.json. Prefer python3/node for correctness, fall
# back to a dependency-light grep/sed if neither is available.
read_version() {
  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import json,sys; print(json.load(open("lattice.json")).get("latticeVersion",""))'
  elif command -v node >/dev/null 2>&1; then
    node -e 'process.stdout.write((require("./lattice.json").latticeVersion)||"")'
  else
    # Grab the first "latticeVersion": "x.y.z" occurrence.
    grep -o '"latticeVersion"[[:space:]]*:[[:space:]]*"[^"]*"' lattice.json \
      | head -n1 \
      | sed -E 's/.*"latticeVersion"[[:space:]]*:[[:space:]]*"([^"]*)".*/\1/'
  fi
}

VERSION="$(read_version)"
if [[ -z "$VERSION" ]]; then
  echo "error: no \"latticeVersion\" pinned in lattice.json; cannot restore release." >&2
  exit 1
fi

RELEASE_NAME="lattice-$VERSION"
RELEASE_BINARY="$ROOT/.lattice/bin/$RELEASE_NAME"

if [[ ! -e "$RELEASE_BINARY" ]]; then
  echo "error: pinned release binary $RELEASE_NAME is not installed at $RELEASE_BINARY." >&2
  echo "       The dev symlink was left unchanged. Bootstrap/install lattice $VERSION first," >&2
  echo "       then re-run this script." >&2
  exit 1
fi

mkdir -p "$ROOT/.lattice/bin"
# Use a relative target to mirror the bootstrap install layout (PRD §9).
ln -sf "$RELEASE_NAME" "$ROOT/.lattice/bin/lattice"

echo "✓ linked .lattice/bin/lattice → release $VERSION"
