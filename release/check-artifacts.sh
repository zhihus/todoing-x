#!/usr/bin/env bash
# Check that a release artifact directory contains the complete expected file
# set and that every file is non-empty.
#
# usage: check-artifacts.sh <artifact-dir>
#
# <artifact-dir> is a directory like release/dist/<VERSION>/<PLATFORM>-<ARCH>
# that holds the artifacts for one platform. The expected names are derived
# from the release manifest in that directory (see packaging.md §5).
set -euo pipefail

DIR=${1:?usage: check-artifacts.sh <artifact-dir>}
[[ -d $DIR ]] || { echo "FAIL: not a directory: $DIR" >&2; exit 1; }
MANIFEST=$(ls "$DIR"/*-release-manifest.json 2>/dev/null | head -n 1)
[[ -n $MANIFEST && -f $MANIFEST ]] || { echo "FAIL: no release manifest found in $DIR" >&2; exit 1; }

# Read the expected artifact names from the manifest.
get_field() {
  python3 -c "import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])" "$MANIFEST" "$1" 2>/dev/null \
    || py -c "import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])" "$MANIFEST" "$1"
}

BIN=$(get_field binary)
PORTABLE=$(get_field portable)
SHA256_FILE=$(get_field sha256_file)

FAIL=0
require_present() {
  local f=$1
  if [[ ! -f "$DIR/$f" ]]; then
    echo "FAIL: missing artifact: $f" >&2
    FAIL=1
  elif [[ ! -s "$DIR/$f" ]]; then
    echo "FAIL: empty artifact: $f" >&2
    FAIL=1
  fi
}

require_present "$BIN"
require_present "$PORTABLE"
require_present "$SHA256_FILE"
# The portable ZIP's sha256 file is a convenience artifact; require it too.
# Its name drops the .zip suffix (see packaging.md §4).
require_present "${PORTABLE%.zip}.sha256"

if (( FAIL )); then
  exit 1
fi
echo "PASS: artifact set complete in $DIR"