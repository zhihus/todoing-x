#!/usr/bin/env bash
# Verify that a release manifest matches the actual files in its directory:
# every named file exists and every recorded SHA256 matches the real file.
#
# usage: verify-manifest.sh <artifact-dir>
#
# This is the automated completeness/integrity check for a release. It exits
# non-zero on any mismatch.
set -euo pipefail

DIR=${1:?usage: verify-manifest.sh <artifact-dir>}
[[ -d $DIR ]] || { echo "FAIL: not a directory: $DIR" >&2; exit 1; }
MANIFEST=$(ls "$DIR"/*-release-manifest.json 2>/dev/null | head -n 1)
[[ -n $MANIFEST && -f $MANIFEST ]] || { echo "FAIL: no release manifest found in $DIR" >&2; exit 1; }

# Do the whole verification in one Python pass for portability.
PY=""
for cand in python3 py python; do
  if command -v "$cand" >/dev/null 2>&1; then PY=$cand; break; fi
done
[[ -n $PY ]] || { echo "FAIL: no Python interpreter found" >&2; exit 1; }

"$PY" - "$DIR" "$MANIFEST" <<'PYEOF'
import hashlib, json, os, sys

art_dir, manifest_path = sys.argv[1], sys.argv[2]
m = json.load(open(manifest_path, encoding="utf-8"))

def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()

errors = []
checks = [
    ("binary", m.get("binary"), m.get("binary_sha256")),
    ("portable", m.get("portable"), m.get("portable_sha256")),
]
for label, name, expected in checks:
    if not name:
        errors.append(f"{label}: missing name in manifest")
        continue
    path = os.path.join(art_dir, name)
    if not os.path.isfile(path):
        errors.append(f"{label}: file not found: {name}")
        continue
    actual = sha256(path)
    if expected and actual != expected.lower():
        errors.append(f"{label}: sha256 mismatch for {name}\n  expected {expected}\n  actual   {actual}")

# The .sha256 sidecar file must agree with the recorded binary hash.
sha_file = m.get("sha256_file")
if sha_file:
    sha_path = os.path.join(art_dir, sha_file)
    if not os.path.isfile(sha_path):
        errors.append(f"sha256_file: not found: {sha_file}")
    else:
        with open(sha_path, encoding="utf-8") as f:
            line = f.read().strip()
        recorded = line.split()[0] if line else ""
        if m.get("binary_sha256") and recorded.lower() != m["binary_sha256"].lower():
            errors.append(f"sha256_file: {sha_file} disagrees with manifest binary_sha256")

if errors:
    print("FAIL: release manifest does not match artifacts", file=sys.stderr)
    for e in errors:
        print("  " + e, file=sys.stderr)
    sys.exit(1)

print(f"PASS: release manifest matches artifacts ({m.get('version')} {m.get('platform')}-{m.get('arch')})")
PYEOF