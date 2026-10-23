# Design

## Source of truth

The release workflow already publishes four GitHub release assets and a
`SHA256SUMS` file. The updater uses the GitHub `releases/latest` API for the
tag, notes link and asset URLs, then downloads exactly one asset whose name
matches the running platform and architecture:

- `plan-my-cabinet-<version>-macos-arm64.tar.gz`
- `plan-my-cabinet-<version>-macos-x86_64.tar.gz`
- `plan-my-cabinet-<version>-linux-x86_64.tar.gz`
- `plan-my-cabinet-<version>-windows-x86_64.zip`

The version comes from `env!("CARGO_PKG_VERSION")`; `semver` compares it with
the release tag after stripping the leading `v`. The check uses a bounded
HTTP timeout and a `User-Agent`. A missing asset, missing `SHA256SUMS`,
malformed JSON or network failure is a check failure, not a crash.

## When it runs

The check starts only in the normal desktop path, after preferences load and
outside capture mode and `--mcp`. `app::update::should_check` excludes debug
builds, a `target/` path component (source-tree builds) and layouts the
installer cannot safely replace. An environment switch
`PMCAB_DISABLE_UPDATE_CHECK` is available for testing and troubleshooting.
The check runs once per process on a worker thread; it never blocks the first
frame. It requests a repaint when it has a result.

The prompt waits for the Welcome screen. If the user has already opened a
project when the check completes, the result is discarded for this run rather
than interrupting work. Choosing Later sets a session-only dismissed flag.

## UI

A dedicated `UpdateHost` owns the check receiver, the available release, the
download job, the progress state and a `ModalChrome`. The normal prompt uses
the existing dialog chrome: versions in the body, a release-notes hyperlink,
**Later** as the cancel action and **Update now** as the primary action.

After Update now, the same chrome shows a reader-style progress panel with a
spinner and the current phase. The back button is **Cancel** and is enabled
only before installation starts; cancelling stops the download without
touching the installed app. Failures return to the normal prompt with the
error under the versions and allow a retry. The whole flow uses the normal
English and Portuguese localization keys.

## Download and verification

A worker thread uses `ureq` with rustls and a long download timeout. It
writes the asset to a unique directory under the system temp directory,
reporting bytes received when the response has a content length. It downloads
`SHA256SUMS`, finds the exact asset line in either `hash  name` or
`hash *name` form, and streams the file through SHA-256. Installation starts
only when the digests match. The temp directory is removed before a successful
restart.

## Platform installation

The running executable determines the layout:

- **macOS**: the executable must be inside `.../Plan My Cabinet.app/Contents/MacOS`.
  The archive is extracted with the system `tar`, the new `.app` is moved
  beside the old bundle, the old bundle is renamed to a unique `.old-<pid>`
  name, and the new bundle takes its place. On any move or launch failure the
  old bundle is restored. `open -n` launches the new bundle.
- **Linux**: the archive is extracted with `tar`. The running executable is
  renamed to a unique `.old-<pid>` sibling, the new executable is copied into
  its place with mode 755, and the new executable is spawned. A failed copy
  restores the old binary.
- **Windows**: PowerShell `Expand-Archive` extracts the zip. The running
  `plan-my-cabinet.exe` is renamed to a unique `.old-<pid>.exe` sibling, the
  new executable is copied into place, and the new executable is spawned. A
  failed copy restores the old executable.

The old process closes only after the replacement and the new launch both
succeed. Stale `.old-*` artifacts from earlier updates are best-effort removed
at the next check. Installation fails with a readable message when the
install location is not writable or is not a release layout.

## Security and failure handling

Only HTTPS GitHub URLs are used. The exact platform asset is selected from the
release's asset list; URLs are not constructed from untrusted text. A missing
or mismatched checksum is fatal. No remote script is executed: archives are
extracted locally with fixed arguments, and only the downloaded app/binary is
replaced. A failed check, download, verification, extraction, replacement or
launch leaves the running app and its project data untouched.

## Tests

Pure unit tests cover version comparison, platform asset selection, GitHub JSON
parsing, checksum line parsing and install-layout detection. Desktop tests
mount the prompt with a fixture release, verify Later dismisses it for the
session, and drive progress cancellation and failure recovery through
test-only host helpers. Network and real installation are never exercised by
tests.
