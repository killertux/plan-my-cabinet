# Tasks

## 1. Update core

- [x] Add `ureq` and `semver` dependencies with a locked resolution.
- [x] Parse the GitHub latest-release response, compare versions, and select the exact platform asset plus `SHA256SUMS`.
- [x] Download the package and checksum in a cancellable worker, verify SHA-256, and report progress.
- [x] Install on macOS bundles, Linux binaries and Windows executables, restoring the old build on failure.
- [x] Add `should_check` guards for debug/source-tree builds, capture, MCP, and the disable switch.

## 2. Startup and UI

- [x] Start the background check after preferences load in the normal desktop path.
- [x] Add the `UpdateHost` state and modal: versions, release-notes link, Update now, Later, progress, cancellation and error.
- [x] Close the old process only after a successful replacement and launch.
- [x] Add English and Portuguese strings and document the update behavior.

## 3. Verification

- [x] Unit tests for version comparison, asset selection, JSON parsing, checksums, layout detection and the Later state.
- [x] Desktop tests for the prompt, Later, progress cancellation and failure recovery without network access.
- [x] `cargo fmt --check`, `cargo check --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings`, and the full test suite pass.
- [ ] Manual install-from-release check on macOS (available/current/offline paths) recorded as remaining manual work.
