# Provider leaks in the UI crates

The rule: each bridge (`ferrite-core/src/providers/claude*`, `providers/codex*`) translates its tool's protocol into the shared `SessionEvent` vocabulary. The engine and the transcript/activity model fold those events, and the UI sees only the shared view. A provider's quirks belong in its bridge, never in the UI.

This audit covers every `Provider::Claude` / `Provider::Codex` in `ferrite-cockpit`, `ferrite-pane`, `ferrite-ui` and `ferrite`, outside test modules, visual-reference scenes and the demo.

## Counts (non-test)

| File | Before | After |
| --- | --- | --- |
| `ferrite-cockpit/src/cockpit.rs` | 41 | 17 |
| `ferrite-cockpit/src/cli_updates.rs` | 14 | 4 |
| `ferrite-pane/src/decision.rs` | 6 | 0 |
| `ferrite-pane/src/pane.rs` | 2 | 0 |
| `ferrite-pane/src/pane/menus.rs` | 2 | 0 |
| `ferrite-cockpit/src/cockpit/server.rs` | 2 | 0 |
| `ferrite/src/lib.rs` | 2 | 2 |
| `ferrite-ui/src/provider.rs` (new) | — | 8 |
| **Total** | **69** | **31** |

The brief's "about 99" also counted test modules. Those are unchanged, apart from the `decision.rs` tests that called the old `context_label` signature.

## Moved

### Behaviour

1. **The permission mode new threads start in.**
   - Before: `cockpit.rs` matched on the provider to read `claude_permission_mode` (defaulting to `"default"`) or `codex_approval_policy`.
   - Now: `Settings::permission_mode_for(provider)` in `ferrite-core/src/settings.rs`. The mode menu's `Default` tag and the decision head both read it.
2. **The decision head's context word.**
   - Before: `decision::context_label(provider, mode, codex_sandbox)` branched on the provider to choose between Codex's sandbox and Claude's mode.
   - Now: `Settings::sandbox_for(provider)` says whether a provider runs in a sandbox (Codex does, `read-only` by default; Claude does not). `context_label(mode, sandbox)` only asks "is there a sandbox?", so it is provider-neutral.
   - `cockpit/decisions.rs` no longer falls back to Claude's mode setting for a Codex thread.
3. **Fallback provider.** Two `unwrap_or(Provider::Claude)` fallbacks in `cockpit.rs` (a typed `/model` with no thread, and the focused pane's choice with nothing open) now use `settings.default_provider`. Before, Claude was assumed whatever the operator had set.
4. **Edit tools in `decision.rs`.**
   - Before: a list of wire tool names (`Edit`, `Write`, `MultiEdit`, `NotebookEdit`, Codex's `fileChange`).
   - Now: `edits_a_file`, which asks the shared tool table (`transcript::tools::policy`) for `ToolKind::Edit | Write`. This is not a `Provider::` use, but it is the same kind of leak.
5. **Codex agent rows (item A).**
   - The Codex bridge now names its spawn calls `tools::AGENT` (`spawnAgent` collaboration calls and 0.153's `subAgentActivity started`) with a `description`.
   - It also links each call's row id to the child's key with an `ActivityEvent::Alias`, the shape Claude's bridge already emits.
   - The UI needed no change. `collabAgentToolCall` left the tool table, which now carries one provider-neutral `AGENT` entry.

### Branding (deduplicated, not behaviour)

Provider names and marks were spelled out in eight places. They are now one module, `ferrite-ui/src/provider.rs`, which provides `title`, `word`, `cli` and `mark`.

That module replaced:

- `decision::provider_name` and `decision::provider_word`;
- cockpit's `provider_label` and `provider_title`;
- `cli_updates`'s `provider_word` and the body of its `name`;
- the "Claude CLI" match in the update row;
- the logomark and ink matches in `provider_group`, the model picker rows, the pane head and `picker_section`.

`Provider::ALL` (core) replaces the seven `[Provider::Claude, Provider::Codex]` literals.

## Left, with reasons

### Branding and naming

These are fine as they are.

- `cockpit.rs` 1433, 1442: `CockpitView::new` defaults a test or demo view to Claude. It is a constructor default, not behaviour.
- `cockpit.rs` 2460–2461: a debug-state snapshot string that lists both providers' limits.
- `cockpit.rs` 4523 (`cli_version` per provider), and settings rows 4968, 4995, 5002, 5150–5151, 5184–5185, 5250–5251:
  - These cover selector ids (`settings-claude-model`), group copy ("Sounds for Claude threads") and the per-provider settings fields.
  - Settings genuinely hold per-provider configuration, so the Settings window lists each provider by name. Folding the ids into `format!` would need `'static` ids, which is churn for no gain.
- `cockpit.rs` 10303–10304, `provider_of_title`: parses a picker section's title back to a provider. It is naming.
- `cli_updates.rs` 87–95: per-provider state storage (`claude`/`codex` fields) behind `state(provider)`. This is bookkeeping, not branching on behaviour.
- `ferrite/src/lib.rs` 155–156: the `--provider claude|codex` command-line argument.
- `ferrite-ui/src/provider.rs`: the branding module itself.

### Leaks not moved (not contained)

- **`decision::shell_source` strips Codex's login-shell wrapper.** It unwraps `commandExecution`'s `/bin/zsh -lc '…'` in the UI.
  - The bridge should hand the decision its command already unwrapped, as Claude's `Bash` sends it.
  - That changes the Codex decision input that is stored, answered and replayed, which goes beyond this pass.
- **`decision::tool_word`** maps `commandExecution` to `Bash` and `fileChange` to `Edit`.
  - The shared table has `policy(name).display`, but that says `Update` for an edit, where the approval card says "edit". Merging them is a wording decision for the owner.
- **Settings fields `claude_permission_mode`, `codex_approval_policy` and `codex_sandbox`.** These are per-provider configuration by design. They are now read through `permission_mode_for` and `sandbox_for` everywhere outside the Settings window.
