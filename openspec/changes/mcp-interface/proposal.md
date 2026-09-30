# Proposal

## Why

Agents such as Claude can design furniture if they can drive the app's validated editing core and check their own work. Today the only way in is the GUI. A headless Model Context Protocol server lets an agent build a complete cabinet — design, stock, cut plan, hardware — look at it, and hand the user a project file to open in the app.

## What Changes

- `plan-my-cabinet --mcp` runs an MCP server over stdio in the same executable, with no window.
- A transport-free agent API (`src/service/`) exposes project, design, template, stock, cut-plan, optimizer, hinge and door operations as JSON tools. Each call is one validated editor transaction; errors carry a stable code, message and hint.
- Agents can see the result: a textual scene description (positions, contacts, overlaps, gaps), CPU-rendered pictures from any view with hidden, isolated or highlighted objects, sheet cut-plan diagrams, and doors shown open.
- The camera, mesh and rasterizer move from the desktop viewport into the library (`src/render/`) so the viewport, the saved thumbnail and agent pictures share one implementation.
- Library additions the tools need: material deletion, placeholder removal, board creation with parent/grain/fit options, material creation with color in one step, blocking wait on the optimizer, template insertion into an open project, Euler helpers, and a shared user data directory.
- Shop handoff (PDF export) stays in the app for now.

## Capabilities

### New Capabilities

- `agent-interface`: Headless MCP server, agent tools, visual feedback, error contract and file safety.

### Modified Capabilities

None; existing editing semantics are unchanged.

## Impact

- New dependencies: `rmcp` (stdio server), `tokio`, `schemars`, `base64`; `resvg` becomes a direct dependency (already in the tree).
- Code: `src/service/`, `src/render/`, `src/read_models/scene_description.rs`, `src/app/mcp*`, small additions in `src/editing/`, `src/catalog/template_setup.rs`, `src/optimize/optimization_worker.rs`, `src/storage/user_dirs.rs`; the viewport and thumbnail import the moved renderer.
- Tests/docs: service, picture and stdio end-to-end tests; `docs/mcp-en.md`, `docs/mcp-pt-BR.md`, README and development notes.
