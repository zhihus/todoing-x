# Release guide

## Build
Run from the repository root:
```sh
release/build.sh [--notes "changelog"] [--skip-build]
```
Windows PowerShell: `release/build.ps1 -Notes "changelog"`.

The build reads version/provenance from Cargo/Git, builds for the host, and writes
the binary, portable ZIP, SHA256 sidecars and JSON manifest to
`release/dist/<VERSION>/<PLATFORM>-<ARCH>/`. Both artifact checks run automatically.
`release/dist/` is gitignored disposable output; reruns replace that platform's
output. `--skip-build` reuses `target/release`.

## Verify and publish
`check-artifacts.sh <dir>` checks required non-empty files;
`verify-manifest.sh <dir>` checks manifest references and SHA256 values.
Both must pass.

GitHub Actions:
- Manual **Run workflow**: validate version, build Windows/Linux/macOS and upload
  workflow artifacts only. No tag or GitHub Release is created.
- Push `v<VERSION>`: validate the tag against `Cargo.toml`, build all runners and
  create/refresh a **draft** GitHub Release. It is never published automatically.

Publish only after explicit approval and review of every platform artifact,
manifest and checksum. Publish the draft manually; never distribute an unverified build.

## Recovery and cache
For recovery, check out the manifest's `source_commit`, rebuild on the matching host,
then verify the downloaded artifact against its manifest. Builds are not byte-reproducible;
source commit plus published-artifact SHA256 establish provenance.

Consumers (e.g. Manifest) cache releases by version/platform under
`<project>/.todoing-cache/`, separate from the private `.todoing/` task store.
See `packaging.md` and Manifest's `resolve-todoing.*`.
