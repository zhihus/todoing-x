# Todoing release packaging rules

This file is the single source of truth for how release artifacts are named,
structured, and stored. The build scripts (`build.sh`, `build.ps1`) and the
verification scripts (`check-artifacts.sh`, `verify-manifest.sh`) all follow
the rules below. If a script and this document disagree, this document wins and
the script is a bug.

## 1. Naming scheme

All release artifacts follow one uniform scheme:

```
todoing-<VERSION>-<PLATFORM>-<ARCH><suffix>
```

Where:
- `<VERSION>` is the SemVer version from `Cargo.toml` (e.g. `1.2.0`).
- `<PLATFORM>` is one of: `windows`, `linux`, `macos`.
- `<ARCH>` is one of: `x64`, `arm64`.
- `<suffix>` depends on the artifact type:
  - plain binary (Windows): `.exe`
  - plain binary (Unix): *(none)*
  - portable package: `-portable.zip`
  - SHA256 file: `.sha256`
  - release manifest: `-release-manifest.json`

Examples for version `1.2.0`:

| Artifact | Name |
|---|---|
| Windows x64 binary | `todoing-1.2.0-windows-x64.exe` |
| Windows x64 portable | `todoing-1.2.0-windows-x64-portable.zip` |
| Windows x64 SHA256 | `todoing-1.2.0-windows-x64.sha256` |
| Windows x64 manifest | `todoing-1.2.0-windows-x64-release-manifest.json` |
| Linux x64 binary | `todoing-1.2.0-linux-x64` |
| Linux x64 portable | `todoing-1.2.0-linux-x64-portable.zip` |
| macOS arm64 binary | `todoing-1.2.0-macos-arm64` |
| macOS arm64 portable | `todoing-1.2.0-macos-arm64-portable.zip` |

The platform/arch token is always `<PLATFORM>-<ARCH>` (e.g. `windows-x64`).
This token is also used as a directory name in the local cache.

## 2. Supported platform matrix

| Platform | Arch | Binary suffix | Notes |
|---|---|---|---|
| `windows` | `x64` | `.exe` | Primary target. Built on Windows. |
| `linux` | `x64` | *(none)* | Built on Linux (CI). |
| `linux` | `arm64` | *(none)* | Built on Linux (CI). |
| `macos` | `x64` | *(none)* | Built on macOS (CI). |
| `macos` | `arm64` | *(none)* | Built on macOS (CI). |

The build script builds for the **current** host platform only. Cross-building
is not performed by the build script; the GitHub Actions workflow
(`.github/workflows/release.yml`) builds each platform on its native runner and
aggregates the artifacts into a single GitHub Release.

## 3. Portable package structure

The portable package is a ZIP archive. When extracted it yields a single folder
named `todoing-<VERSION>-<PLATFORM>-<ARCH>-portable/` containing:

```
todoing-1.2.0-windows-x64-portable/
├── todoing.exe          # the release binary (renamed to plain "todoing[.exe]")
├── README.md            # brief startup instructions
└── VERSION.txt          # version, source commit, SHA256 of the binary
```

Rules:
- The portable package contains **only** the binary and documentation. It must
  not contain a user task store (`.todoing/`), a configuration file, or any
  absolute paths.
- The binary inside the package is named `todoing.exe` (Windows) or `todoing`
  (Unix), not the full release name, so it can be run directly after extraction.
- `VERSION.txt` records the version, the source Git commit, and the SHA256 of
  the binary, so a consumer can verify provenance without network access.
- The package is not tied to any absolute path. It works after extraction into
  any directory, with no installer, no admin rights, no Rust, no Python, and no
  PATH changes.

## 4. SHA256 files

Each binary and each portable ZIP gets a `.sha256` file. The format is the
standard `sha256sum` output:

```
<64-hex-sha256>  <artifact-basename>
```

The `.sha256` file for the plain binary is named
`todoing-<VERSION>-<PLATFORM>-<ARCH>.sha256` and contains the hash of the plain
binary. The portable ZIP's hash is recorded inside the release manifest
(`portable_sha256`) and is also written to
`todoing-<VERSION>-<PLATFORM>-<ARCH>-portable.sha256` for convenience.

## 5. Release manifest

The release manifest is a JSON document describing one platform's release
artifacts. The template is `manifest.template.json`. The build script fills it
in and writes it as
`todoing-<VERSION>-<PLATFORM>-<ARCH>-release-manifest.json`.

The manifest records:
- `format_version` (currently `1`)
- `version`
- `source_commit` (full Git SHA)
- `source_branch`
- `created` (UTC ISO-8601 timestamp)
- `notes` (brief changelog)
- `platform`, `arch`
- `binary`, `binary_sha256`
- `portable`, `portable_sha256`
- `sha256_file`

`verify-manifest.sh` checks that every file named in the manifest exists and
that every recorded SHA256 matches the actual file. This is the automated
completeness check for a release.

## 6. Canonical storage

The canonical store for all official releases is the **GitHub Releases** of the
Todoing repository. Each release (one tag = one version) carries, for every
supported platform:
- the plain binary,
- the portable ZIP,
- the `.sha256` files,
- the release manifest JSON.

Binaries are **never committed to Git history**. They are attached to GitHub
Releases only. The local build output (`release/dist/`) and the local release
cache are working copies, not the canonical store.

## 7. Local cache layout

Consumers (e.g. Manifest) keep a local cache of downloaded releases, separated
by version and platform, and **never** mixed with the user task store
(`.todoing/`). The convention is:

```
<cache-root>/releases/<VERSION>/<PLATFORM>-<ARCH>/
    todoing-<VERSION>-<PLATFORM>-<ARCH>.exe
    todoing-<VERSION>-<PLATFORM>-<ARCH>-portable.zip
    todoing-<VERSION>-<PLATFORM>-<ARCH>.sha256
    todoing-<VERSION>-<PLATFORM>-<ARCH>-release-manifest.json
<cache-root>/portable/<VERSION>/<PLATFORM>-<ARCH>/
    todoing.exe
    README.md
    VERSION.txt
```

`<cache-root>` defaults to `<project>/.todoing-cache/` (see the Manifest
resolver). The `releases/` subtree holds raw downloaded artifacts; the
`portable/` subtree holds an extracted portable package ready to run.

## 8. Reproducibility

The build is **not** claimed to be reproducible. Rust embeds build timestamps
and environment-dependent metadata, so two builds of the same commit can differ
byte-for-byte even when functionally identical. Provenance is established by
the source commit recorded in the release manifest and by SHA256 verification
against the manifest, not by byte-identical rebuilds. Do not advertise
reproducible builds unless a reproducible-build toolchain is actually in place.