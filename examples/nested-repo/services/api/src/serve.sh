#!/bin/sh
# Stand-in for the API process: serves the frontend bundle as it is on disk.
# Read at run time, never copied at build time — see the README caveat.
set -e
echo "api serving:"
cat ../../frontend/packages/site/dist/bundle.js
