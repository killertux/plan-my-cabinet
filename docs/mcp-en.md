# Using Plan My Cabinet from an AI agent (MCP)

Plan My Cabinet can run as a [Model Context Protocol](https://modelcontextprotocol.io)
server. An agent such as Claude then designs furniture with the same checked
edits as the app: it builds the boards, declares stock, plans the cuts, adds
hinges and doors, looks at pictures of the result, and saves a `.pmcab` file
that you open in the app.

The server has no window. It runs on your computer and reads and writes only
the files the agent is asked to open or save.

## Set it up

Use the installed program with the `--mcp` option.

**Claude Code**:

```sh
claude mcp add plan-my-cabinet -- "/Users/you/Applications/Plan My Cabinet.app/Contents/MacOS/plan-my-cabinet" --mcp
```

On Linux the program is `~/.local/bin/plan-my-cabinet`; on Windows
`%LOCALAPPDATA%\Programs\PlanMyCabinet\plan-my-cabinet.exe`.

**Claude Desktop** (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "plan-my-cabinet": {
      "command": "/Users/you/Applications/Plan My Cabinet.app/Contents/MacOS/plan-my-cabinet",
      "args": ["--mcp"]
    }
  }
}
```

Options:

| Option | Meaning |
|---|---|
| `--open FILE.pmcab` | Open a project when the server starts. |
| `--catalog-dir DIR` | Folder with your hinge catalog packs. Default: the app's `catalogs` folder. |
| `--user-data-dir DIR` | The app's data folder (recent projects, catalogs). Saved projects show up on the app's Welcome screen. |
| `--language en\|pt-BR` | Language of the standard material names. |

## What the agent can do

About seventy tools, in groups:

- **Project**: new, open, save, close, settings (units, kerf, cut fee), currency, undo and redo.
- **Design**: materials, boards (create, copy, resize, move, rotate, place one against another), assemblies, reference hardware, and the Base, Wall and Drawers templates.
- **Seeing the result**: a written description of the model (positions, which boards touch, which ones **intersect**), pictures from any angle with parts hidden, isolated or highlighted, drawings of each sheet's cut plan, and a door shown open.
- **Stock and cut plan**: sheets and offcuts, the sheets still needed, automatic placement, manual placement, diagnostics, and the cut-plan optimizer.
- **Hardware**: hinge catalogs, hinges on doors, and doors that swing.

Exporting the shop PDF is not available yet: open the saved file in the app and
use **Handoff**.

## Ask for something

For example:

> Design a kitchen base cabinet 800 mm wide, 720 mm high and 580 mm deep with
> two doors. Buy the sheets it needs at R$ 320 each, optimize the cut plan,
> add hinges, show me the front and the inside without the doors, and save it
> to ~/Documents/kitchen-base.pmcab.

The agent works in steps and checks each one. It asks before it overwrites a
file or discards unsaved work.

## Good to know

- Numbers are millimetres. The agent can also write `"60 cm"` or `"23 5/8 in"`.
  Values that do not fit a whole micrometre need its explicit consent.
- Each change is one undo step while the server runs; the file on disk only
  changes on save.
- Hinge positions are reference information from the catalog, like in the app.
  Check them against the manufacturer's sheet before drilling.
- The server prints nothing on its standard output except protocol messages.
  Problems go to standard error.
