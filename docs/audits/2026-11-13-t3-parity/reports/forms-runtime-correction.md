# Required provider forms runtime correction

## Result

**PERM-01 remains open.** I exercised a real headless Ferrite window, production Claude adapter and controlled local provider, with no real model. The fixture reached a required two-string-field elicitation form and a separate provider approval request through the public runtime. Required validation stayed visible and recovery by Cancel worked. In the captured headless frames, the labels and inputs were not visible. The field container selectors had nonzero bounds, however, and source constructs both the labels and `Input` children. I cannot distinguish missing/clipped paint from a headless capture/repaint artifact, so the screenshot alone does not establish that those elements are absent in the product. Public clicks at the selector containers followed by typing left focus in the main composer; no valid form answer was submitted. The actual input bounds were not separately exposed by a selector.

The approval branch worked for explicit Deny. The card showed the intended request/command and waiting state. `interaction_required=true` disabled Allow once, so the enabled Deny control sent the matching provider response. The controlled provider then completed and Ferrite showed Done.

## Controlled fixture and protocol

The driver and fake provider are in [`evidence/forms-correction`](../evidence/forms-correction/). The driver created a new Store and scratch checkout under `/tmp/ferrite-audit-forms-fm77wemq`, set PATH shims for Claude/Codex, and launched the existing `target/debug/ferrite`. It used a fresh fake provider process for each thread. No source files, builds, or tests were changed or run.

The provider sent `control_request` IDs `forms-runtime-required-form` (elicitation form) and `forms-runtime-approval` (Bash `can_use_tool`). The schema was valid for the supported adapter: `type=object`, a `properties` object with two `type=string` entries, `minLength=2`, and both names in the `required` array. `decision list --thread 1` confirmed both normalized fields and their required flags. The fake waited for the UI response before continuing; it recorded all control frames in `provider-frames.jsonl`.

Source review found no established cause for the invisible contents. [`form_card`](../../../../crates/ferrite-cockpit/src/cockpit/subagents.rs) creates an `InputState` for each string field, wraps a label, description and `Input::new(input)` in the `form-field-{id}` selector, and puts the body in a max-height scrolling container. The retained `window find` outputs report bounds `affirmation=(504,350,704,80)` and `confirmation=(504,450,704,80)`; neither parent selector is zero-sized. `window click --selector form-field-affirmation` clicked its parent center `(856,390)`, not a separately measured input element. A raw click at `(700,410)` also did not focus the field. The available evidence does not show whether those positions were over the actual nested inputs after layout/clipping.

## Observed branches

- Empty form submission via `window click --selector request-submit-1-1` retained the form and displayed `Affirmation is required` in `03-required-validation.png`.
- The selectors `form-field-affirmation` and `form-field-confirmation` had nonzero bounds, but the fields/labels did not appear in the captured frames `02-form.png` or `07-coordinate-form.png`. Clicking the first selector (its parent center) and typing `YesForm`, Tab, `SecondForm`, Tab, Enter left the request pending. Those strings appeared in the main composer (`04-after-form-submit.png`, `05-approval.png`) appended to `COMPOSER_SENTINEL`; they were not sent to the provider as an answer. A second public click at `(700,410)` and typing `YESSENTINEL` likewise appeared in the composer (`08-coordinate-click.png`). This is a bounded headless focus-routing failure for those clicks; it does not prove the real input elements are missing. I used no internal field mutation.
- Clicking `form-cancel` sent the exact elicitation response `{"action":"cancel","content":null}` for `forms-runtime-required-form`. This resolved that request and let the fixture reach approval. Cancellation is recovery from the blocked form, not a successful answer.
- The approval card showed `Controlled harmless permission request`, command `printf controlled-audit-only`, and `Needs your input` while the thread was Working (`05-approval.png`). `Allow once` was disabled because the request required interaction. Clicking `request-deny-1-2` sent `{"behavior":"deny","message":"The operator denied this and will say what to do instead.","interrupt":true}` for `forms-runtime-approval`; the fixture then emitted its completion marker and Ferrite showed Done (`06-after-deny.png`). No real command ran.
- The distinct main composer draft was not preserved: the attempted answer text appended to it. The exact request IDs and response frames, plus command stdout/stderr, are retained in the evidence directory.

## Reproduction commands

All commands were run through the copied fixture driver (shell history is also in `commands.jsonl`):

```sh
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py prepare
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py launch
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py run thread start --cwd /tmp/ferrite-audit-forms-fm77wemq/work --provider claude --ready --focus --title 'Forms correction audit'
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py run window type COMPOSER_SENTINEL
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py run agent prompt 1 'Run the controlled forms fixture.'
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py run window click --selector request-submit-1-1
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py run window click --selector form-field-affirmation
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py run window type YesForm
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py run window key tab
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py run window type SecondForm
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py run window key tab
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py run window key enter
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py run window click --selector form-cancel
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py run window click --selector request-deny-1-2
python3 docs/audits/2026-11-13-t3-parity/evidence/forms-correction/drive.py close
```

The exact sequence, including the separate coordinate retry, is in `evidence/forms-correction/commands.jsonl`. Case classifications and screenshot index are in `evidence/forms-correction/cases.json`.

## Coverage limits and cleanup

Still unverified: valid form values reaching the correct provider request, actual nested-input focus, field focus recovery, Tab traversal and Enter submit, async questions during streaming, child decisions while Main is focused, and Codex sandbox truthfulness (PS-01). The headless screenshots do not establish whether the controls are unpainted, clipped, or affected by incomplete repaint; no source cause was found. The parent-container clicks did not focus the inputs and typed text went to the composer, so the supported public path remains unverified. Native UI remains outside this correction.

This overlaps the existing [`coverage-quality-03`](coverage-quality.md#coverage-quality-03) verification gap: the required-form test accepts rejection of either typed-invalid or untouched-empty input, without checking typed value or composer preservation. That issue is the known test weakness. This runtime pass does not add a separate finding for absent fields because it did not isolate product rendering from headless capture behavior.

Both owned threads were closed. The owned Ferrite server/headless process groups and fake provider PIDs were checked absent; `cleanup.json` records the result. The final process-table search found no fixture processes.
