# Remote connections verification

The native app connected through an actual, nonroot OpenSSH daemon at 1440×900, 1000×700, and 800×600. Each flow added and verified an environment, confirmed a new host key, answered a native passphrase prompt, recovered from invalid input and a failed connection, disconnected and reconnected, selected the remote machine, opened a remote project and thread, edited and saved a file, ran a remote terminal command, removed the environment, and proved the Server remained available after the window disconnected.

The [machine-readable results](evidence/native-final/summary.json) record the successful checks, binary hashes, SSH file-operation counts, remote terminal connection/cwd, fixture provider turns, and process cleanup. The [visual gallery](gallery.md) shows each changed surface. The native renderer supplied the pixels; these are screenshots, not design mockups.

## Reproduce

On macOS, with the system OpenSSH tools available:

```sh
cargo build -p ferrite -p ferrite-core --features ferrite/headless --bin ferrite --bin ferrited
python3 scripts/verify-remote-connections.py --output /tmp/ferrite-remote-review verify
```

Use a fresh output directory. The script creates task-owned keys, SSH configuration, Stores, and a project under `/tmp`; it does not edit the operator's SSH configuration or replace HOME. It checks an explicit simulated Codex launcher before sending the fixture prompt. It only stops recorded processes whose start identities still match.

## Narrow checks

| Coverage | Result | Evidence |
| --- | --- | --- |
| SSH target parsing, authentication, setup/bootstrap, file operations, wire fixtures, bridge ownership | 41 passed | [core](evidence/core-tests.log) |
| Terminal process arguments, working directory, input, and local behavior | 8 passed | [terminal](evidence/terminal-tests.log) |
| Settings, connection state, remote files, previews, recovery, and terminals | 116 passed | [UI](evidence/ui-tests.log) |
| Empty remote workspace Escape and machine lifecycle | 26 passed | [machine regressions](evidence/empty-workspace-tests.log) |
| Actual remote Files/Terminal keyboard doors | Passed | [shortcut run](evidence/remote-shortcuts-and-editor-tests.log) |
| Selected file text replaced through platform input and saved | Passed | [editor regression](evidence/editor-selection-test.log) |
| Files empty state after compact-layout correction | Passed | [Files check](evidence/files-empty-layout-test.log) |
| Binary/read-failure guidance and terminal failure/Retry after recovery-layout correction | 2 passed | [recovery checks](evidence/recovery-layout-tests.log) |
| Production app and native driver after final source edit | Passed | [typecheck](evidence/typecheck.log), [build](evidence/build.log) |

Commands used; the later targeted checks cover source corrections found by the native flow and visual review. Repeated coverage is not counted as additional unique tests.

```sh
cargo nextest run -p ferrite-core -E 'test(machine::) | test(file_operations) | test(protocol::tests::every_gen1_message_has_a_frozen_fixture_that_round_trips_byte_for_byte) | test(protocol::tests::every_frozen_fixture_still_decodes) | test(server::tests::a_window_on_a_machine_drives_its_threads_through_the_bridge)'
cargo nextest run -p ferrite-terminal -E 'test(session::tests)'
cargo nextest run -p ferrite-ui -p ferrite-frame -p ferrite-pane -p ferrite-cockpit -E 'test(settings::) | test(cockpit::tests::settings_window::) | test(cockpit::tests::machines::) | test(machines::tests::) | test(cockpit::tests::files::) | test(cockpit::tools::tree::tests::) | test(cockpit::tools::save::tests::) | test(cockpit::tests::terminal::) | test(cockpit::tools::terminal::tests::) | test(preview::tests::) | test(attachments::tests::) | test(link_target::tests::)'
cargo nextest run -p ferrite-cockpit -E 'test(cockpit::tests::machines::)'
cargo nextest run -p ferrite-cockpit -E 'test(cockpit::tests::machines::remote_files_and_terminal_open_through_their_keyboard_shortcuts) | test(cockpit::tests::files::typing_replaces_selected_file_text_through_platform_input)'
cargo nextest run -p ferrite-cockpit -E 'test(cockpit::tests::files::typing_replaces_selected_file_text_through_platform_input)'
cargo nextest run -p ferrite-cockpit -E 'test(cockpit::tests::files::the_door_opens_files_with_its_empty_word_and_the_tree)'
cargo nextest run -p ferrite-cockpit -E 'test(cockpit::tests::files::remote_files_failure_and_binary_never_fall_back_to_readable_local_text) | test(cockpit::tests::terminal::a_shell_that_cannot_start_says_so_and_retries)'
cargo check -p ferrite --features headless
bash scripts/tests/package-server.sh
```

The combined shortcut/editor run passed the shortcut regression and exposed a test-helper click that cleared the selection. The subsequent editor run uses focused platform input without that extra click and passed. The shell packaging fixture passed after its last edit. It was repeated narrowly to retain [packaging proof](evidence/packaging-check.log) after the trail reviewer found the missing log. The [release YAML check](evidence/release-yaml-check.log) parsed the workflow and checked its three server targets and release fan-in; it did not execute GitHub Actions. Existing compiler warnings remain. The full suite is left to CI.

## Verification boundaries

- The transport and remote terminal were real SSH; the final provider response was a labeled protocol fixture. Client and server ran on this Mac and shared its filesystem. SSH ingress recorded snapshot/read/write operations; separate unit tests used local/remote path collisions to prove that remote tools do not fall back to local I/O.
- The native matrix used a prepared task-owned `ferrited`. Cold installation, checksums, archive contents, atomic publication, and failures were exercised with shell fixtures. Downloading public assets requires a future tagged release running the new release jobs. No release was published by this task.
- The release targets are Linux x64/arm64 GNU and macOS arm64. Linux builds use Ubuntu 22.04; arbitrary cloud distributions and their service/linger configuration were not live-tested. Native Windows client authentication has a batch-attach proof and tests, but no live Windows run was available. A Windows remote PC needs a compatible Unix environment such as WSL.
- Remote Files supports text editing. Remote image/binary viewing offers terminal guidance; remote prose opens as source. Changes remains local-only because its implementation still operates on a local checkout.
- A debug fixture initially lost provider selection to an installed Codex CLI and made one real OpenAI turn in the isolated fixture project. Later runs require an explicit verified launcher before prompting. No Anthropic model was used. Failed attempts remain local and are excluded from the public gallery.
- The initial matrix binary includes the changes later committed as `05dc35ef`. Its recorded repository HEAD was still `7cef3de1` for the first two sizes because those edits were uncommitted when the fixture was prepared; the binary SHA256 was identical at all three sizes. The targeted post-review Files recapture uses the source at `92087e9e` and records its separate binary hash.
- The extended recovery pass at 1000×700 uses actual SSH file failures, conflict detection and overwrite/reload, plus a remote shell `exit 23` followed by Retry. Only the setup-warning diagnostic is fault-injected on the actual setup SSH stderr; its text explicitly identifies it as synthetic. Final recovery captures use `5e5aa24c` after wrapping long diagnostics.

## Reviews

Independent OpenAI reviewers checked the architecture, T3 fit, backend, connection lifecycle, file/terminal routing, and comments. Material findings were corrected before the final flow. The fresh visual review found one clipped Files instruction at 1000×700; it was constrained and wrapped before the targeted recapture. The final visual verdict and the decision-trail review are recorded in [reviews.md](reviews.md). The user's model constraint excluded cross-provider Claude reviewers.
