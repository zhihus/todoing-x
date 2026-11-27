#!/usr/bin/env bash
# Build Todoing release artifacts for the current host platform.
#
# One command produces, under release/dist/<VERSION>/<PLATFORM>-<ARCH>/:
#   - the plain release binary
#   - the portable ZIP package
#   - the .sha256 files
#   - the release manifest JSON
# and then verifies them with check-artifacts.sh and verify-manifest.sh.
#
# This script never tags, publishes, or pushes. See RELEASE.md.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

NOTES="Release build."
SKIP_BUILD=0
while (($#)); do
  case "$1" in
    --notes) NOTES=$2; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    -h|--help)
      echo "usage: build.sh [--notes TEXT] [--skip-build]" >&2
      exit 0
      ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

fail() { echo "FAIL: $1" >&2; exit 1; }

# Portable SHA256: sha256sum (Linux/Git Bash) or shasum -a 256 (stock macOS).
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

# --- Version / provenance -------------------------------------------------
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
[[ -n $VERSION ]] || fail "cannot determine version from Cargo.toml"
COMMIT=$(git rev-parse HEAD 2>/dev/null) || fail "not a git checkout (need source commit)"
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
CREATED=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# --- Platform / arch detection --------------------------------------------
OS_RAW=$(uname -s)
case "$OS_RAW" in
  Linux) PLATFORM=linux ;;
  Darwin) PLATFORM=macos ;;
  MINGW*|MSYS*|CYGWIN*) PLATFORM=windows ;;
  *) fail "unsupported OS: $OS_RAW" ;;
esac
ARCH_RAW=$(uname -m)
case "$ARCH_RAW" in
  x86_64|amd64) ARCH=x64 ;;
  aarch64|arm64) ARCH=arm64 ;;
  *) fail "unsupported arch: $ARCH_RAW" ;;
esac
PLATFORM_ARCH="${PLATFORM}-${ARCH}"
[[ $PLATFORM == windows ]] && EXE=".exe" || EXE=""
# Precompute platform-specific examples so heredocs need no command substitution.
if [[ $PLATFORM == windows ]]; then
  RUN_EXAMPLE="./todoing.exe --help"
  HASH_EXAMPLE="certutil -hashfile todoing.exe SHA256"
else
  RUN_EXAMPLE="./todoing --help"
  HASH_EXAMPLE="sha256sum todoing"
fi

# --- Python interpreter (for ZIP creation) --------------------------------
PY=""
for cand in python3 py python; do
  if command -v "$cand" >/dev/null 2>&1; then PY=$cand; break; fi
done
[[ -n $PY ]] || fail "no Python interpreter found (needed to create the portable ZIP)"

# --- Build ----------------------------------------------------------------
if [[ $SKIP_BUILD -eq 0 ]]; then
  echo ">> cargo build --release"
  cargo build --release
fi
SRC_BIN="target/release/todoing${EXE}"
[[ -f $SRC_BIN ]] || fail "build output not found: $SRC_BIN"

# --- Output layout (absolute paths, so subshell cd cannot break them) -----
OUT="$ROOT/release/dist/${VERSION}/${PLATFORM_ARCH}"
STAGE="$ROOT/release/dist/${VERSION}/.stage-${PLATFORM_ARCH}"
rm -rf "$OUT" "$STAGE"
mkdir -p "$OUT"

BIN_NAME="todoing-${VERSION}-${PLATFORM_ARCH}${EXE}"
PORTABLE_NAME="todoing-${VERSION}-${PLATFORM_ARCH}-portable.zip"
SHA256_NAME="todoing-${VERSION}-${PLATFORM_ARCH}.sha256"
PORTABLE_SHA256_NAME="todoing-${VERSION}-${PLATFORM_ARCH}-portable.sha256"
MANIFEST_NAME="todoing-${VERSION}-${PLATFORM_ARCH}-release-manifest.json"

# --- Plain binary ---------------------------------------------------------
cp "$SRC_BIN" "$OUT/$BIN_NAME"

# --- Portable package -----------------------------------------------------
PORTABLE_DIR="$STAGE/todoing-${VERSION}-${PLATFORM_ARCH}-portable"
mkdir -p "$PORTABLE_DIR"
cp "$SRC_BIN" "$PORTABLE_DIR/todoing${EXE}"

BIN_SHA256=$(sha256_of "$OUT/$BIN_NAME")

export P_DIR="$PORTABLE_DIR" P_VERSION="$VERSION" P_PLATFORM_ARCH="$PLATFORM_ARCH" \
       P_COMMIT="$COMMIT" P_BRANCH="$BRANCH" P_EXE="$EXE" P_BIN_SHA="$BIN_SHA256" \
       P_CREATED="$CREATED" P_RUN_EXAMPLE="$RUN_EXAMPLE" P_HASH_EXAMPLE="$HASH_EXAMPLE"
"$PY" - <<'PYEOF'
import os
e = os.environ
d = e["P_DIR"]
exe = e["P_EXE"]
version_txt = (
    f"Todoing {e['P_VERSION']}\n"
    f"platform: {e['P_PLATFORM_ARCH']}\n"
    f"source_commit: {e['P_COMMIT']}\n"
    f"source_branch: {e['P_BRANCH']}\n"
    f"binary: todoing{exe}\n"
    f"binary_sha256: {e['P_BIN_SHA']}\n"
    f"built: {e['P_CREATED']}\n"
)
readme = f"""# Todoing {e['P_VERSION']} (portable, {e['P_PLATFORM_ARCH']})

A self-contained Todoing build. No installer, no admin rights, no Rust, no
Python, and no PATH changes are required.

## Run it

Extract this archive into any directory, then run the binary directly:

```
{e['P_RUN_EXAMPLE']}
```

To use it from a project without installing anything, point the project at the
binary with the `TODOING_BIN` environment variable:

```
TODOING_BIN=/path/to/this/folder/todoing{exe}
```

## Provenance

- Version: {e['P_VERSION']}
- Source commit: {e['P_COMMIT']}
- Binary SHA256: {e['P_BIN_SHA']}

Verify the binary before first use:

```
{e['P_HASH_EXAMPLE']}
```

The package contains no task store and no configuration. A project's
`.todoing/` store is created by the project on first use, not shipped here.
"""
with open(os.path.join(d, "VERSION.txt"), "w", encoding="utf-8") as f:
    f.write(version_txt)
with open(os.path.join(d, "README.md"), "w", encoding="utf-8") as f:
    f.write(readme)
print("wrote VERSION.txt and README.md")
PYEOF

# Create the ZIP with Python's zipfile (reliable across Windows/Linux).
(
  cd "$STAGE"
  "$PY" - "$PORTABLE_DIR" "$OUT/$PORTABLE_NAME" <<'PYEOF'
import sys, os, zipfile
src, dst = sys.argv[1], sys.argv[2]
root = os.path.basename(os.path.normpath(src))
with zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as z:
    for dirpath, dirnames, filenames in os.walk(src):
        for fn in filenames:
            full = os.path.join(dirpath, fn)
            arc = os.path.relpath(full, os.path.dirname(src))
            z.write(full, arc.replace(os.sep, "/"))
print("zipped", dst)
PYEOF
)

# --- SHA256 files ---------------------------------------------------------
PORTABLE_SHA256=$(sha256_of "$OUT/$PORTABLE_NAME")
printf '%s  %s\n' "$BIN_SHA256" "$BIN_NAME" > "$OUT/$SHA256_NAME"
printf '%s  %s\n' "$PORTABLE_SHA256" "$PORTABLE_NAME" > "$OUT/$PORTABLE_SHA256_NAME"

# --- Release manifest -----------------------------------------------------
MANIFEST="$OUT/$MANIFEST_NAME"
export M_VERSION="$VERSION" M_COMMIT="$COMMIT" M_BRANCH="$BRANCH" \
       M_CREATED="$CREATED" M_NOTES="$NOTES" M_PLATFORM="$PLATFORM" \
       M_ARCH="$ARCH" M_BIN="$BIN_NAME" M_BIN_SHA="$BIN_SHA256" \
       M_PORTABLE="$PORTABLE_NAME" M_PORTABLE_SHA="$PORTABLE_SHA256" \
       M_SHA_FILE="$SHA256_NAME"
"$PY" - "$MANIFEST" <<'PYEOF'
import json, os, sys
e = os.environ
m = {
  "format_version": 1,
  "version": e["M_VERSION"],
  "source_commit": e["M_COMMIT"],
  "source_branch": e["M_BRANCH"],
  "created": e["M_CREATED"],
  "notes": e["M_NOTES"],
  "platform": e["M_PLATFORM"],
  "arch": e["M_ARCH"],
  "binary": e["M_BIN"],
  "binary_sha256": e["M_BIN_SHA"],
  "portable": e["M_PORTABLE"],
  "portable_sha256": e["M_PORTABLE_SHA"],
  "sha256_file": e["M_SHA_FILE"],
}
with open(sys.argv[1], "w", encoding="utf-8") as f:
    json.dump(m, f, indent=2, ensure_ascii=False)
    f.write("\n")
print("manifest", sys.argv[1])
PYEOF

rm -rf "$STAGE"

echo ""
echo ">> artifacts in $OUT:"
( cd "$OUT" && ls -1 )
echo ""
echo ">> running checks"
bash release/check-artifacts.sh "$OUT"
bash release/verify-manifest.sh "$OUT"
echo ""
echo "OK: release artifacts built and verified for ${VERSION} (${PLATFORM_ARCH})"