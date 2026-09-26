#!/usr/bin/env python3
"""Reapply OpenCode agent routing after `openspec update` (including --force)."""

from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
COMMANDS = ROOT / ".opencode" / "commands"
ROUTES = {
    "explore": "architect",
    "propose": "architect",
    "update": "architect",
    "new": "architect",
    "continue": "architect",
    "ff": "architect",  # Present in some OpenSpec workflow profiles.
    "onboard": "architect",
    "apply": "builder",
    "verify": "reviewer",
    "sync": "builder",
    "archive": "builder",
    "bulk-archive": "builder",  # Optional administrative workflow.
}
REQUIRED = {"explore", "propose", "update", "new", "continue", "apply", "verify", "sync", "archive"}


def route(path: Path, agent: str) -> bool:
    original = path.read_bytes()
    text = original.decode("utf-8")
    match = re.match(r"\A---(\r?\n)(.*?)(\r?\n)---(?=\r?\n|\Z)", text, re.DOTALL)
    if match is None:
        raise ValueError(f"Missing YAML frontmatter: {path}")

    newline = match.group(1)
    metadata = match.group(2)
    metadata = newline.join(line for line in metadata.split(newline) if not line.startswith("agent:"))
    updated = text[: match.start(2)] + metadata + (newline if metadata else "") + f"agent: {agent}" + text[match.end(2) :]
    if updated == text:
        return False
    path.write_bytes(updated.encode("utf-8"))
    return True


def main() -> None:
    missing = [name for name in sorted(REQUIRED) if not (COMMANDS / f"opsx-{name}.md").is_file()]
    if missing:
        raise SystemExit(f"Missing OpenSpec commands: {', '.join(missing)}")

    for name, agent in ROUTES.items():
        path = COMMANDS / f"opsx-{name}.md"
        if path.is_file():
            print(f"{path.relative_to(ROOT)} → {agent}" + (" (updated)" if route(path, agent) else ""))


if __name__ == "__main__":
    main()
