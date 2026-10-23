# Proposal

## Why

Users install Plan My Cabinet from GitHub release archives and currently have to notice a new release and rerun the installer themselves. The app should tell them when a newer release exists at startup and, with one confirmation, install it and restart.

## What Changes

- **Startup check.** A release build that was installed from a release package checks the GitHub latest-release API once at startup, in the background. Debug builds, source-tree builds, captures and `--mcp` do not check. A failed or offline check is silent and never blocks the app.
- **Prompt.** When a newer version exists, a modal shows the current and available versions, a release-notes link, and **Update now** / **Later**. Later dismisses the prompt for the current session; the next startup checks again.
- **Install and restart.** Update now downloads the platform package for the running architecture, requires and verifies `SHA256SUMS`, extracts it, replaces the installed `.app` / executable, and launches the new version. Download and install run in the background with visible progress and cancellation before installation. Failures leave the current build usable and report the reason.
- **Offline promise.** Core editing, saving, exports and MCP remain fully offline. The startup check is the only network access and is skipped in development and unsupported layouts.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `desktop-and-localization`: the offline requirement explicitly permits the optional update check, and a new requirement covers the prompt, verified install and restart behavior.

## Impact

- Desktop: new `app::update` module (GitHub check, download, checksum, platform install, restart), startup wiring in `main`, modal state in `DesktopApp`, English and Portuguese strings.
- Dependencies: `ureq` with rustls for HTTPS and `semver` for version comparison. Release packaging and `SHA256SUMS` already exist; no project format, export or MCP change.
- Platform coverage: macOS `.app` bundles, Linux binaries, and Windows executables installed by the release scripts. Development and source-tree builds never replace themselves.
