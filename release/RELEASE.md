# Todoing release & recovery guide

This guide explains how to build release artifacts, how to publish a release,
and how to recover (rebuild/verify) a specific version later. It is the
operational companion to `packaging.md` (the naming/structure rules).

## 1. Building release artifacts (one command)

From the repository root:

```sh
release/build.sh            # build for the current host platform
```

or, on native Windows PowerShell:

```powershell
release\build.ps1           # build for the current host platform
```

What the build does, in order:
1. Reads the version from `Cargo.toml` and the source commit/branch from Git.
2. Runs `cargo build --release`.
3. Detects the host platform/arch and derives the artifact names per
   `packaging.md`.
4. Copies the binary to `release/dist/<VERSION>/<PLATFORM>-<ARCH>/` under its
   release name.
5. Assembles the portable package (binary + `README.md` + `VERSION.txt`) and
   zips it.
6. Computes SHA256 for the binary and the portable ZIP and writes the `.sha256`
   files.
7. Fills in the release manifest from `manifest.template.json` and writes
   `todoing-<VERSION>-<PLATFORM>-<ARCH>-release-manifest.json`.
8. Runs `check-artifacts.sh` and `verify-manifest.sh` and fails the build if
   either check fails.

The build is idempotent: re-running it regenerates `release/dist/` from scratch
for the current platform. `release/dist/` is gitignored and is a working copy,
not the canonical store.

### Options

```sh
release/build.sh --notes "Brief changelog text"   # set the manifest notes
release/build.sh --skip-build                      # reuse an existing target/release build
```

## 2. Verifying artifacts

Two independent checks guard every release:

```sh
release/check-artifacts.sh release/dist/<VERSION>/<PLATFORM>-<ARCH>
#   -> asserts the expected file set is present and non-empty

release/verify-manifest.sh release/dist/<VERSION>/<PLATFORM>-<ARCH>
#   -> asserts every file named in the manifest exists and every recorded
#      SHA256 matches the actual file
```

Both exit non-zero on any mismatch. They are also run automatically at the end
of `build.sh` / `build.ps1`.

## 3. Publishing a release (manual, requires approval)

Publishing is a separate, explicitly approved step. It is **not** done by the
build scripts. The steps are:

1. Ensure the working tree is clean and the version in `Cargo.toml` matches the
   release you intend to publish.
2. Build artifacts for every supported platform (locally or via the GitHub
   Actions workflow).
3. Create the tag: `git tag v<VERSION>` (one release = one tag = one version).
4. Create a GitHub Release for the tag and attach, for each platform:
   - the plain binary,
   - the portable ZIP,
   - the `.sha256` files,
   - the release manifest JSON.
5. Push the tag and the release.

> **Do not publish, tag, or push without explicit approval.** The build scripts
> never perform these steps.

## 4. GitHub Actions

`.github/workflows/release.yml` builds artifacts on each supported platform's
native runner. It has two triggers with different behavior:

- **Manual `workflow_dispatch`** (with a `version` and optional `notes`):
  validates the `version` input against `Cargo.toml`, builds the artifacts,
  and uploads them as workflow artifacts only. It does **not** create a tag
  and does **not** create a GitHub Release.
- **`v*` tag push**: validates the tag against `Cargo.toml`, builds the
  artifacts, and creates/refreshes a **draft** GitHub Release for that tag.
  It never publishes the draft.

The workflow never creates or pushes tags, never publishes a release, and
never pushes commits. Review a draft release, then publish it manually. See
the workflow file for the exact job layout.

## 5. Recovering a specific version

To rebuild or verify a past release:

1. Check out the exact commit recorded in the release manifest
   (`source_commit`): `git checkout <source_commit>`.
2. Run `release/build.sh` (or `build.ps1`) on the matching platform.
3. Compare the produced binary's SHA256 against the `binary_sha256` recorded in
   the release manifest.

Because builds are **not** reproducible (see `packaging.md` §8), a rebuild of
the same commit may differ byte-for-byte from the published binary. The rebuild
is functionally equivalent; provenance is established by the recorded commit and
by verifying the *published* artifact's SHA256 against the manifest, not by
byte-identical rebuilds. To use a past release without rebuilding, download the
published artifacts from the GitHub Release and verify them with
`verify-manifest.sh`.

## 6. Local cache (consumer side)

Consumers such as Manifest keep a local cache of downloaded releases, separated
by version and platform and never mixed with the user task store (`.todoing/`).
See `packaging.md` §7 for the layout and the Manifest resolver
(`scripts/resolve-todoing.sh` / `resolve-todoing.py`) for how the cache is
populated and how the binary is resolved.