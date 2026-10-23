# Spec Delta

## MODIFIED Requirements

### Requirement: Desktop workflows SHALL operate offline on macOS arm64

The application SHALL provide the core project, board, assembly, allocation, and local export workflows as a macOS arm64 desktop application. These workflows SHALL not require sign-in or connectivity. A saved versioned project file SHALL reopen locally without changing model values. Native file-picker cancellation SHALL return to the current project without losing edits. Linux builds and cross-platform GUI transfer remain unverified and are not first-release requirements.

An optional startup update check MAY use the network in a release build installed from a release package. It SHALL run in the background, SHALL not block or disable any core workflow when it fails or is offline, and SHALL not run in debug builds, source-tree builds, captures, or the `--mcp` server.

#### Scenario: Complete a disconnected workflow
- **WHEN** the macOS application runs without network access
- **THEN** a user can create a project, edit boards, arrange an assembly, review allocations, save, reopen, and generate supported local outputs

#### Scenario: Cancel Save As
- **WHEN** a user cancels the platform file picker during Save As
- **THEN** the current project and its unsaved edits remain available without reporting a successful save

#### Scenario: Offline update check
- **WHEN** a release build starts without a network connection
- **THEN** the update check fails quietly and every editing, saving, export and MCP workflow remains available

## ADDED Requirements

### Requirement: Update checks SHALL require consent and verified packages

A release build installed from a release package SHALL check the project's GitHub latest release once at startup and SHALL NOT prompt when the running version is current. When a newer version exists, the application SHALL show a modal naming the current and available versions, offer a release-notes link, and provide **Update now** and **Later** actions. **Later** SHALL dismiss the prompt for the current session only. **Update now** SHALL download the package matching the running operating system and architecture, require and verify the release's `SHA256SUMS`, and install only after verification. The download and install SHALL run off the UI thread, SHALL remain cancellable before installation begins, and SHALL report failures without replacing the working installation. A successful update SHALL launch the new version and close the old one.

#### Scenario: Update available
- **WHEN** the running version is 0.8.0 and the latest release is 0.9.0
- **THEN** the startup modal identifies both versions and offers Update now and Later

#### Scenario: Later
- **WHEN** the user chooses Later
- **THEN** the prompt closes for this run and the next startup checks again

#### Scenario: Checksum mismatch
- **WHEN** the downloaded package does not match the release's SHA256SUMS
- **THEN** the installer refuses it, the current version keeps running, and the modal explains the failure

#### Scenario: Current version
- **WHEN** the latest release is not newer than the running version
- **THEN** no update prompt appears and no download is started

#### Scenario: Development build
- **WHEN** the application runs from a debug or source-tree build
- **THEN** it does not check for or install updates
