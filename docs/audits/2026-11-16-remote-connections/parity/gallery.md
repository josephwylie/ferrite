# Remote workflow screenshot gallery

69 original native captures, plus eight side-by-side comparisons. The parent inspected the captured surfaces; [visual verdict](visual-review.md) records the differences and [verification](verification.md) records runtime assertions and their limits.

This is the current review gallery. The [earlier 69-capture SSH gallery](../gallery.md) is historical. Screenshot counts are not proof by themselves: native-6's new-thread claim was rejected during visual review and corrected before this gallery was accepted.

## Start here

- [Remote link comparison](comparisons/remote-link-comparison.png)
- [SSH comparison](comparisons/ssh-comparison.png)
- [Chat comparison](comparisons/chat-comparison.png)
- [Actual Codex response](real-codex/real-codex-completed-1470x923.png)
- [Compact SSH validation](compact-final/04-ssh-error-640x420.png)
- [Functional summary](native-7/summary.json)

The comparison has T3 on the left and Ferrite on the right. Matching 1470×923 logical viewports at DPR 2 are placed together without rescaling. The original files and their hash/provenance sidecars are linked below.

## Side-by-side comparisons

| Surface | Comparison | Provenance |
| --- | --- | --- |
| Remote Link | [Open](comparisons/remote-link-comparison.png) | [JSON](comparisons/remote-link-comparison.json) |
| Ssh | [Open](comparisons/ssh-comparison.png) | [JSON](comparisons/ssh-comparison.json) |
| Validation | [Open](comparisons/validation-comparison.png) | [JSON](comparisons/validation-comparison.json) |
| Attachment | [Open](comparisons/attachment-comparison.png) | [JSON](comparisons/attachment-comparison.json) |
| Chat | [Open](comparisons/chat-comparison.png) | [JSON](comparisons/chat-comparison.json) |
| Changes | [Open](comparisons/changes-comparison.png) | [JSON](comparisons/changes-comparison.json) |
| Files | [Open](comparisons/files-comparison.png) | [JSON](comparisons/files-comparison.json) |
| Terminal | [Open](comparisons/terminal-comparison.png) | [JSON](comparisons/terminal-comparison.json) |

The connection dialog mirrors T3's composition. Chat, Files, Changes and terminal use Ferrite's native renderer and retain visible differences. These images do not establish pixel identity or complete T3 product parity. The T3 chat reference uses a synthetic local project and an actual GPT-6.1-Sol response; the final Ferrite matrix uses an explicit protocol fixture, with actual Codex proved separately over SSH.

## Final Ferrite native workflow

| Capture | Original | Metadata |
| --- | --- | --- |
| 01 connections empty | [Image](native-7/01-connections-empty-1470x923.png) | [JSON](native-7/01-connections-empty-1470x923.json) |
| 02 add remote default | [Image](native-7/02-add-remote-default-1470x923.png) | [JSON](native-7/02-add-remote-default-1470x923.json) |
| 03 remote validation | [Image](native-7/03-remote-validation-1470x923.png) | [JSON](native-7/03-remote-validation-1470x923.json) |
| 04 add ssh | [Image](native-7/04-add-ssh-1470x923.png) | [JSON](native-7/04-add-ssh-1470x923.json) |
| 05 ssh include autocomplete | [Image](native-7/05-ssh-include-autocomplete-1470x923.png) | [JSON](native-7/05-ssh-include-autocomplete-1470x923.json) |
| 06 ssh autocomplete selected | [Image](native-7/06-ssh-autocomplete-selected-1470x923.png) | [JSON](native-7/06-ssh-autocomplete-selected-1470x923.json) |
| 07 ssh validation | [Image](native-7/07-ssh-validation-1470x923.png) | [JSON](native-7/07-ssh-validation-1470x923.json) |
| 08 ssh connecting cancel | [Image](native-7/08-ssh-connecting-cancel-1470x923.png) | [JSON](native-7/08-ssh-connecting-cancel-1470x923.json) |
| 09 ssh failure | [Image](native-7/09-ssh-failure-1470x923.png) | [JSON](native-7/09-ssh-failure-1470x923.json) |
| 10 ssh authentication | [Image](native-7/10-ssh-authentication-1470x923.png) | [JSON](native-7/10-ssh-authentication-1470x923.json) |
| 11 ssh connected | [Image](native-7/11-ssh-connected-1470x923.png) | [JSON](native-7/11-ssh-connected-1470x923.json) |
| 20 remote pairing rejected | [Image](native-7/20-remote-pairing-rejected-1470x923.png) | [JSON](native-7/20-remote-pairing-rejected-1470x923.json) |
| 21 remote link connected | [Image](native-7/21-remote-link-connected-1470x923.png) | [JSON](native-7/21-remote-link-connected-1470x923.json) |
| link changes | [Image](native-7/link-changes-1470x923.png) | [JSON](native-7/link-changes-1470x923.json) |
| link changes staged | [Image](native-7/link-changes-staged-1470x923.png) | [JSON](native-7/link-changes-staged-1470x923.json) |
| link changes unstaged | [Image](native-7/link-changes-unstaged-1470x923.png) | [JSON](native-7/link-changes-unstaged-1470x923.json) |
| link disconnected | [Image](native-7/link-disconnected-1470x923.png) | [JSON](native-7/link-disconnected-1470x923.json) |
| link existing thread attachment ready | [Image](native-7/link-existing-thread-attachment-ready-1470x923.png) | [JSON](native-7/link-existing-thread-attachment-ready-1470x923.json) |
| link existing thread completed | [Image](native-7/link-existing-thread-completed-1470x923.png) | [JSON](native-7/link-existing-thread-completed-1470x923.json) |
| link existing thread streaming tool | [Image](native-7/link-existing-thread-streaming-tool-1470x923.png) | [JSON](native-7/link-existing-thread-streaming-tool-1470x923.json) |
| link file read | [Image](native-7/link-file-read-1470x923.png) | [JSON](native-7/link-file-read-1470x923.json) |
| link file saved | [Image](native-7/link-file-saved-1470x923.png) | [JSON](native-7/link-file-saved-1470x923.json) |
| link files tree | [Image](native-7/link-files-tree-1470x923.png) | [JSON](native-7/link-files-tree-1470x923.json) |
| link new thread attachment ready | [Image](native-7/link-new-thread-attachment-ready-1470x923.png) | [JSON](native-7/link-new-thread-attachment-ready-1470x923.json) |
| link new thread completed | [Image](native-7/link-new-thread-completed-1470x923.png) | [JSON](native-7/link-new-thread-completed-1470x923.json) |
| link new thread streaming tool | [Image](native-7/link-new-thread-streaming-tool-1470x923.png) | [JSON](native-7/link-new-thread-streaming-tool-1470x923.json) |
| link reconnected | [Image](native-7/link-reconnected-1470x923.png) | [JSON](native-7/link-reconnected-1470x923.json) |
| link remote thread | [Image](native-7/link-remote-thread-1470x923.png) | [JSON](native-7/link-remote-thread-1470x923.json) |
| link remove confirmation | [Image](native-7/link-remove-confirmation-1470x923.png) | [JSON](native-7/link-remove-confirmation-1470x923.json) |
| link removed | [Image](native-7/link-removed-1470x923.png) | [JSON](native-7/link-removed-1470x923.json) |
| link terminal | [Image](native-7/link-terminal-1470x923.png) | [JSON](native-7/link-terminal-1470x923.json) |
| ssh changes | [Image](native-7/ssh-changes-1470x923.png) | [JSON](native-7/ssh-changes-1470x923.json) |
| ssh changes staged | [Image](native-7/ssh-changes-staged-1470x923.png) | [JSON](native-7/ssh-changes-staged-1470x923.json) |
| ssh changes unstaged | [Image](native-7/ssh-changes-unstaged-1470x923.png) | [JSON](native-7/ssh-changes-unstaged-1470x923.json) |
| ssh disconnected | [Image](native-7/ssh-disconnected-1470x923.png) | [JSON](native-7/ssh-disconnected-1470x923.json) |
| ssh existing thread attachment ready | [Image](native-7/ssh-existing-thread-attachment-ready-1470x923.png) | [JSON](native-7/ssh-existing-thread-attachment-ready-1470x923.json) |
| ssh existing thread completed | [Image](native-7/ssh-existing-thread-completed-1470x923.png) | [JSON](native-7/ssh-existing-thread-completed-1470x923.json) |
| ssh existing thread streaming tool | [Image](native-7/ssh-existing-thread-streaming-tool-1470x923.png) | [JSON](native-7/ssh-existing-thread-streaming-tool-1470x923.json) |
| ssh file read | [Image](native-7/ssh-file-read-1470x923.png) | [JSON](native-7/ssh-file-read-1470x923.json) |
| ssh file saved | [Image](native-7/ssh-file-saved-1470x923.png) | [JSON](native-7/ssh-file-saved-1470x923.json) |
| ssh files tree | [Image](native-7/ssh-files-tree-1470x923.png) | [JSON](native-7/ssh-files-tree-1470x923.json) |
| ssh new thread attachment ready | [Image](native-7/ssh-new-thread-attachment-ready-1470x923.png) | [JSON](native-7/ssh-new-thread-attachment-ready-1470x923.json) |
| ssh new thread completed | [Image](native-7/ssh-new-thread-completed-1470x923.png) | [JSON](native-7/ssh-new-thread-completed-1470x923.json) |
| ssh new thread streaming tool | [Image](native-7/ssh-new-thread-streaming-tool-1470x923.png) | [JSON](native-7/ssh-new-thread-streaming-tool-1470x923.json) |
| ssh reconnected | [Image](native-7/ssh-reconnected-1470x923.png) | [JSON](native-7/ssh-reconnected-1470x923.json) |
| ssh remote thread | [Image](native-7/ssh-remote-thread-1470x923.png) | [JSON](native-7/ssh-remote-thread-1470x923.json) |
| ssh remove confirmation | [Image](native-7/ssh-remove-confirmation-1470x923.png) | [JSON](native-7/ssh-remove-confirmation-1470x923.json) |
| ssh removed | [Image](native-7/ssh-removed-1470x923.png) | [JSON](native-7/ssh-removed-1470x923.json) |
| ssh terminal | [Image](native-7/ssh-terminal-1470x923.png) | [JSON](native-7/ssh-terminal-1470x923.json) |

## Actual Codex over SSH

| Capture | Original | Metadata |
| --- | --- | --- |
| real codex attachment ready | [Image](real-codex/real-codex-attachment-ready-1470x923.png) | [JSON](real-codex/real-codex-attachment-ready-1470x923.json) |
| real codex before prompt | [Image](real-codex/real-codex-before-prompt-1470x923.png) | [JSON](real-codex/real-codex-before-prompt-1470x923.json) |
| real codex completed | [Image](real-codex/real-codex-completed-1470x923.png) | [JSON](real-codex/real-codex-completed-1470x923.json) |
| real codex running | [Image](real-codex/real-codex-running-1470x923.png) | [JSON](real-codex/real-codex-running-1470x923.json) |

## Compact connection dialogs

| Capture | Original | Metadata |
| --- | --- | --- |
| 01 settings | [Image](compact-final/01-settings-640x420.png) | [JSON](compact-final/01-settings-640x420.json) |
| 02 remote link | [Image](compact-final/02-remote-link-640x420.png) | [JSON](compact-final/02-remote-link-640x420.json) |
| 03 ssh | [Image](compact-final/03-ssh-640x420.png) | [JSON](compact-final/03-ssh-640x420.json) |
| 04 ssh error | [Image](compact-final/04-ssh-error-640x420.png) | [JSON](compact-final/04-ssh-error-640x420.json) |

## Actual T3 native reference

| Capture | Original | Metadata |
| --- | --- | --- |
| 01 environments | [Image](t3/01-environments.jpg) | [JSON](t3/01-environments.json) |
| 02 add remote link | [Image](t3/02-add-remote-link.jpg) | [JSON](t3/02-add-remote-link.json) |
| 03 add ssh | [Image](t3/03-add-ssh.jpg) | [JSON](t3/03-add-ssh.json) |
| 04 ssh validation | [Image](t3/04-ssh-validation.jpg) | [JSON](t3/04-ssh-validation.json) |
| 05 attachment ready | [Image](t3/05-attachment-ready.jpg) | [JSON](t3/05-attachment-ready.json) |
| 06 chat streaming | [Image](t3/06-chat-streaming.jpg) | [JSON](t3/06-chat-streaming.json) |
| 07 chat completed | [Image](t3/07-chat-completed.jpg) | [JSON](t3/07-chat-completed.json) |
| 08 chat tool activity | [Image](t3/08-chat-tool-activity.jpg) | [JSON](t3/08-chat-tool-activity.json) |
| 09 changes | [Image](t3/09-changes.jpg) | [JSON](t3/09-changes.json) |
| 10 files tree | [Image](t3/10-files-tree.jpg) | [JSON](t3/10-files-tree.json) |
| 11 file read | [Image](t3/11-file-read.jpg) | [JSON](t3/11-file-read.json) |
| 12 terminal | [Image](t3/12-terminal.jpg) | [JSON](t3/12-terminal.json) |
