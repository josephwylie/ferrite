# Independent reviews

All reviewers used OpenAI models. The user prohibited Anthropic models, so configured Claude seats and the cross-provider judge were omitted. Independent attempts on OpenAI models do not constitute cross-provider diversity.

## Architecture and source

Before implementation, how explorers traced the existing core SSH/Server boundary and the native UI/T3 workflow. Why investigators traced repository history and searched the available document source. Two architecture reviewers compared explicit setup with installation during bridge dials. The [architecture](architecture.md) separates direct historical evidence from inference and records unavailable evidence categories. The [design](design.md) records the chosen interface and rejected alternative.

Subsequent reviews found and corrected Windows interactive-to-batch handoff, damaged managed-runtime selection, hidden forms under Settings search, duplicate edits, unsaved remote connection actions, remote preview local-I/O fallback, sticky read errors, fatal descendant listing errors, missing Retry, and unsafe machine switches with dirty buffers. Comment review removed redundant narration. The live native flow then found the empty-workspace Escape assumption, an old local-only tool gate, and selected-text input registration. Each source correction received narrow verification.

## Visual review

Reviewer: **gpt-6-astra, high**. It inspected the final native screenshots at 1440×900, 1000×700, and 800×600 before reading the builder's design documents. It verified native typography/material, Settings hierarchy, form fields, inline failures, SSH host confirmation, filled masked authentication, connection lifecycle, remote identity, Files, and Terminal. This is an incumbent native extension; new-world seed, PRODUCT, and raster-comp requirements do not apply.

Initial disposition: **fix**. The empty Files instruction clipped at 1000×700.

The source constrained the empty column and allowed the instruction to wrap. The same reviewer scored the updated screenshots at all three sizes.

Final disposition: **ship**.

> Resolved: the Files empty-editor instruction is fully readable within its column in the updated 1440×900 and 1000×700 tree captures and 800×600 empty-editor capture. Compact layouts wrap cleanly onto two lines. No fix-induced regressions observed.

No material fix remained. Preserve the native hierarchy, restrained controls, readable SSH verification, masked authentication, and explicit machine identity.

The extended recovery pass found the same long-line clipping in file guidance and terminal errors. After the source constrained and wrapped those messages, two narrow tests, typecheck, build, and the actual recovery flow passed. A fresh **gpt-6-astra, high** reviewer inspected all ten final recovery images before the builder's documents. Its disposition was **ship**, with no material fixes. It confirmed complete guidance, visible Retry/Reload/Overwrite, intentional status ellipsis, and truthful fixture labels.

## Decision-trail audit

Reviewer: **gpt-6-astra, high**. It checked all 16 rows present at its snapshot, resolved every local evidence pointer, matched the 41/8/116/26 test counts to the logs, and distinguished the passing shortcut test from the failing combined run and later passing editor test. Readable session metadata corroborated that how/why exploration and two architecture attempts preceded implementation. It did not edit source or run tests.

The reviewer flagged missing gallery/review documents, missing standalone packaging proof, incomplete recovery-state screenshot coverage, and the pending compact-layout verdict. The gallery and this report supply the review documents. Packaging and release YAML now have retained command output. The compact-layout verdict is recorded above. Targeted recovery captures supplement the three-size happy-path matrix and are identified separately in the gallery.

The confirmed local session file contains encrypted message and tool bodies. The reviewer could inspect identity, timestamps and call metadata, source, and artifacts; it could not perform a complete transcript-content audit. The root used its conversation context to append omitted decisions. Reused typecheck/build log paths establish the current green result; they do not independently prove every earlier historical checkpoint.

Keep these acceptance limits visible in the PR:

- The native flow proves actual localhost SSH with a prepared server and simulated provider. It does not prove cold installation on a cloud distribution or live Windows behavior.
- Public installer assets require a future tagged release. Linux user-service/linger behavior depends on the host.
- One debug attempt made an unintended real OpenAI turn in the isolated fixture project. The final harness guards the explicit fixture launcher before prompting.
