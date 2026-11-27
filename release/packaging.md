# Release packaging contract

Single source of truth for artifact names, contents, integrity and storage.
Scripts must conform; disagreement is a script defect.

## Targets and names
`todoing-<VERSION>-<PLATFORM>-<ARCH><suffix>`
- Version: SemVer from `Cargo.toml`; platform: `windows|linux|macos`;
  architecture: `x64|arm64`.
- Binary: `.exe` on Windows, no suffix on Unix.
- Portable ZIP: `-portable.zip`; SHA256 sidecar: `.sha256`;
  manifest: `-release-manifest.json`.
- Local scripts build the current host only. CI builds on native Windows, Ubuntu
  and macOS runners; no cross-compilation.

## Portable archive
One top-level `todoing-<VERSION>-<PLATFORM>-<ARCH>-portable/` directory:
- `todoing[.exe]` — executable;
- `README.md` — run/verification instructions;
- `VERSION.txt` — version, source commit, binary SHA256.

No task store, config or absolute paths. Extraction needs no installer, admin
rights, Rust, Python or PATH changes.

## Integrity
Each platform directory contains the binary, portable ZIP, one SHA256 sidecar
per artifact (`<64-hex-digest>  <basename>`) and
`todoing-...-release-manifest.json`.

Manifest fields: `format_version`, `version`, `source_commit`, `source_branch`,
`created` (UTC ISO-8601), `notes`, `platform`, `arch`, `binary`,
`binary_sha256`, `portable`, `portable_sha256`, `sha256_file`.
`check-artifacts.sh` checks required non-empty files;
`verify-manifest.sh` checks manifest references and hashes. Both must pass.

## Storage and cache
GitHub Releases is canonical. Each release includes all platform binaries,
portable ZIPs, sidecars and manifests. Never commit binaries to Git.
`release/dist/` and consumer caches are disposable copies.

Consumer cache:
- `<cache-root>/releases/<VERSION>/<PLATFORM>-<ARCH>/` — downloaded artifacts;
- `<cache-root>/portable/<VERSION>/<PLATFORM>-<ARCH>/` — extracted package.

Default: `<project>/.todoing-cache/`. Keep separate from private `.todoing/`.

## Reproducibility
Builds are not claimed byte-reproducible. Verify published files against manifest
SHA256 and retain the source commit. Do not claim reproducibility without a
verified reproducible toolchain.
