# Spec Delta

## Purpose

Let an AI agent build a complete piece of furniture headlessly through the Model Context Protocol, with the same validation as the app and a way to see the result.

## ADDED Requirements

### Requirement: The executable SHALL offer a headless MCP mode

`plan-my-cabinet --mcp` SHALL serve MCP over stdin/stdout without opening a window or requiring a graphics device. Standard output SHALL carry only protocol messages. Closing stdin SHALL end the process successfully.

#### Scenario: Start and list tools
- **WHEN** a client initializes the server and lists tools
- **THEN** it receives the server instructions and tools covering project, design, templates, stock, cut planning, optimization, hinges, doors and visual feedback

### Requirement: Each tool change SHALL be a validated editor transaction

Tools that change the project SHALL use the existing editing commands, SHALL leave the project unchanged when they fail, and SHALL report the resulting revision, whether anything changed and how many undo steps were recorded. A supplied `expected_revision` that differs from the current revision SHALL be refused.

#### Scenario: Invalid manual sheet layout
- **WHEN** an agent places two boards so they overlap on a sheet
- **THEN** the call fails with `invalid_placement`, lists the affected sheets, and no placement changes

### Requirement: Failures SHALL be explained to the agent

A failing tool SHALL return an error result with a stable `code`, an English `message`, and where useful a `hint` and structured `details`, instead of a protocol error.

#### Scenario: Ambiguous name
- **WHEN** two boards share a name and a tool names one of them
- **THEN** the result is `ambiguous_ref` with the candidate ids

### Requirement: The agent SHALL be able to see what it built

The server SHALL describe the geometry in text — object positions and orientations, contacts, overlaps and small gaps — and SHALL render PNG pictures of the model from named or free views with hidden, isolated or highlighted objects, pictures of each used sheet with its numbered cuts, and a door opened to a given angle.

#### Scenario: Look inside a cabinet
- **WHEN** an agent renders the iso view with the doors hidden
- **THEN** it receives a picture without the doors and a legend mapping the numbers in the picture to board names

### Requirement: Files SHALL be protected

The server SHALL write only on `save_project`, to an absolute `.pmcab` path, SHALL NOT replace another existing file without `overwrite`, and SHALL NOT discard unsaved changes when opening or creating a project without `discard_changes`.

#### Scenario: Unsaved work
- **WHEN** the open project has unsaved changes and the agent calls `new_project`
- **THEN** the call fails with `unsaved_changes` and the project stays open

### Requirement: Optimization SHALL be bounded and explicit

Optimization SHALL stop within the requested time, SHALL return ranked candidates with an id, and SHALL apply a candidate only on request and only if the manufacturing inputs are unchanged since the search.

#### Scenario: Stale search
- **WHEN** a board is resized after a search and the agent applies a candidate from it
- **THEN** the call fails with `stale_search`
