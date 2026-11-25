"""Write the hub's `.claude/settings.json` shaped like the operator's: CRLF,
Claude's own plugins, a remote marketplace with autoUpdate, a local one,
and keys that are this computer's own.

  hub_settings.py <hub home>
"""
import json
import os
import sys

home = sys.argv[1]
local = os.path.join(home, "local-tools")
os.makedirs(os.path.join(local, ".claude-plugin"), exist_ok=True)
manifest = {"name": "local-tools", "owner": {"name": "me"}, "plugins": [{"name": "mine", "source": "./mine", "description": "mine"}]}
open(os.path.join(local, ".claude-plugin", "marketplace.json"), "w").write(json.dumps(manifest))
settings = {
    "permissions": {"allow": ["Read", "Bash(cargo nextest:*)"], "deny": ["Artifact"], "defaultMode": "acceptEdits"},
    "model": "opus[1m]",
    "effortLevel": "xhigh",
    "enabledPlugins": {
        "cc-plugin-telemetry@builtin": True,
        "cc-plugin-plugin-authoring@builtin": True,
        "pstack@cursor-plugins": True,
        "mine@local-tools": True,
    },
    "extraKnownMarketplaces": {
        "cursor-plugins": {"source": {"source": "github", "repo": "cursor/plugins"}, "autoUpdate": True},
        "local-tools": {"source": {"source": "directory", "path": local}},
    },
    "hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "powershell -File " + os.path.join(home, ".claude", "hooks", "nextest.ps1")}]}]},
    "statusLine": {"type": "command", "command": "powershell -File " + os.path.join(home, ".claude", "statusline.ps1")},
    "env": {"VITEST_MAX_FORKS": "2"},
    "theme": "dark",
}
path = os.path.join(home, ".claude", "settings.json")
open(path, "wb").write((json.dumps(settings, indent=2) + "\n").replace("\n", "\r\n").encode())
print("wrote", path)
