#!/usr/bin/env bash
# dev-link.sh — dev-binary hotswap for contributors working ON Lattice (PRD §13).
#
# Repo-local tooling (NOT a lattice subcommand). Builds the dev binary and points the
# stable ./.lattice/bin/lattice symlink at the freshly compiled ./target/debug/lattice.
# Non-destructive: only the `lattice` symlink is replaced; the versioned release binary
# (lattice-<version>) is never touched.
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

echo "◇ building dev binary (cargo build) ..."
cargo build

DEV_BINARY="$ROOT/target/debug/lattice"
if [[ ! -x "$DEV_BINARY" ]]; then
  echo "error: dev binary not found at $DEV_BINARY after build." >&2
  exit 1
fi

mkdir -p "$ROOT/.lattice/bin"
ln -sf "$DEV_BINARY" "$ROOT/.lattice/bin/lattice"

echo "✓ linked .lattice/bin/lattice → dev (target/debug/lattice)"
