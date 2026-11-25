"""Shape a temporary home like the operator's, for a live Setup sync.

  seed_home.py hub <home>       Windows-style: CRLF text, junctions from
                                .claude/skills into .agents/skills
  seed_home.py machine <home>   Linux-style: LF text, the folders the first
                                table made of those links

Writes only under <home>. Never reads any other home.
"""
import json
import os
import subprocess
import sys
from pathlib import Path

flavour, home = sys.argv[1], Path(sys.argv[2])
hub = flavour == "hub"
eol = "\r\n" if hub else "\n"


def text(rel, body):
    path = home / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(body.replace("\n", eol).encode("utf-8"))


def raw(rel, data):
    path = home / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def link(rel, target):
    path = home / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    if hub:
        subprocess.run(["cmd", "/c", "mklink", "/J", str(path), str(home / target)], check=True, capture_output=True)
    else:
        os.symlink(home / target, path)


REUI = {
    "SKILL.md": "---\nname: reui\ndescription: ReUI design rules\n---\n# ReUI\n\nUse the theme tokens.\n",
    "rules/tokens.md": "# Tokens\n\n- surface\n- accent\n",
}
ASK = {
    "SKILL.md": "---\nname: ask-matt\ndescription: Ask before building\n---\n# Ask Matt\n",
    "PHASE-BOUNDARIES.md": "# Phases\n\n1. Ask\n2. Build\n",
}

for name, files in (("reui", REUI), ("ask-matt", ASK)):
    for rel, body in files.items():
        text(f".agents/skills/{name}/{rel}", body)
        if hub:
            continue
        # The first table followed the link and made a folder of it here.
        text(f".claude/skills/{name}/{rel}", body)
if hub:
    link(".claude/skills/reui", ".agents/skills/reui")
    link(".claude/skills/ask-matt", ".agents/skills/ask-matt")
else:
    text(".claude/skills/ask-matt/NOTES.md", "Written on the Machine, in the folder only.\n")

side = "pc" if hub else "linux"
text(f".claude/skills/{side}-skill/SKILL.md", f"---\nname: {side}-skill\ndescription: made on {side}\n---\n# {side}\n")
text(".claude/skills/both-edit/SKILL.md", f"---\nname: both-edit\ndescription: edited on each side\n---\n# Edited on {side}\n")
text(".claude/skills/synced/state.json", json.dumps({"synced": side}))
if hub:
    raw(".claude/skills/pc-skill/Thumbs.db", b"\x00\x01thumbs")
    raw(".claude/skills/pc-skill/desktop.ini", b"[.ShellClassInfo]\r\n")
    text(".claude/CLAUDE.md", "# Rules\n\nBe brief.\n")
    text(".claude/commands/team.md", "Run the team.\n")
else:
    raw(".claude/skills/linux-skill/.DS_Store", b"\x00\x00\x00\x01Bud1")
    text(".claude/statusline.sh", "#!/bin/sh\necho linux\n")

here = str(home)
if hub:
    claude_json = {
        "userID": "pc-user",
        "projects": {"C:\\work": {"allowedTools": []}},
        "mcpServers": {
            "docs": {"command": "npx", "args": ["-y", "@acme/docs-mcp"]},
            "pc-tool": {"command": "D:\\tools\\pc-tool.exe"},
            "from-plugin": {"command": here + "\\.claude\\plugins\\cache\\acme\\server.exe"},
        },
    }
else:
    claude_json = {"userID": "linux-user", "mcpServers": {"search": {"command": "search-mcp"}}}
raw(".claude.json", json.dumps(claude_json, indent=2).encode())

if hub:
    config = f"""model = "gpt-6.2-astra"
model_reasoning_effort = "high"
notify = ['{here}\\bin\\notify.exe']

[projects.'C:\\work']
trust_level = "trusted"

[marketplaces.cursor-plugins]
last_updated = "2026-11-20T09:12:44Z"
last_revision = "4f1c2a9e0b7d"
source_type = "git"
source = "https://github.com/cursor/plugins.git"

[marketplaces.local-kit]
source_type = "local"
source = '\\\\?\\{here}\\kit'

[plugins."pstack@cursor-plugins"]
enabled = true

[mcp_servers.docs]
command = "npx"
args = ["-y", "@acme/docs-mcp"]
"""
    kit = home / "kit" / ".claude-plugin"
    kit.mkdir(parents=True, exist_ok=True)
    (kit / "marketplace.json").write_text(json.dumps({"name": "local-kit", "owner": {"name": "me"}, "plugins": []}))
else:
    config = f"""model = "gpt-6.2-sol"
notify = ["{here}/bin/notify.sh"]

[projects."/home/joseph/work"]
trust_level = "trusted"
"""
text(".codex/config.toml", config)

if not hub:
    settings = {
        "enabledPlugins": {"cc-plugin-telemetry@builtin": True},
        "model": "sonnet",
        "theme": "light",
        "statusLine": {"type": "command", "command": "~/.claude/statusline.sh"},
    }
    raw(".claude/settings.json", (json.dumps(settings, indent=2) + "\n").encode())
    plugins = home / ".claude" / "plugins"
    plugins.mkdir(parents=True, exist_ok=True)
    (plugins / "installed_plugins.json").write_text(json.dumps({"version": 2, "plugins": {}}, indent=2))
print("seeded", flavour, home)
