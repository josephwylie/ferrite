# Threads

A Thread is one agent conversation in a folder. The user starts one, sends prompts,
watches the transcript fill in its Pane, renames it, parks it (its Session ends, its
history stays) and revives it later in place.

## Sub-features

- `thread-start` starts a Claude Thread in the Project and shows it in the sidebar and Pane.
- `thread-prompt` sends a prompt; the reply appears in the transcript and attention goes `done`.
- `thread-read` returns the transcript tail as text.
- `thread-rename` changes the title in the sidebar and Pane header.
- `thread-park` removes it from the open list; it shows under `Parked`.
- `thread-revive` restarts the parked Thread with the same id and its history.

## How to get to it (user POV)

- `cmd-t` / `cmd-n` or `New thread` in the sidebar, then type in the prompt box.
- `cmd-w` parks, `cmd-o` brings back the last parked.
- Double-click the Pane title to rename.
- Scripting: `ferrite thread start|rename|close|revive`, `ferrite agent prompt|read|wait`.

## Driving it with fv

Preconditions:

- Baseline run; `fv doctor` all `ok`; no Threads open.

- **Start from the composer (keyboard).** On a fresh run the Pane shows `What should we build in verify?`. Run `fv click --selector composer-block`, `fv type "Reply with exactly the word MANGO. Use no tools."`, `fv shot composer-typed` (text in the box, send arrow lit; on Linux `fv text composer-block` reads the draft), then `fv key enter`. `fv cli -- thread list` shows Thread `1` `working`; `fv cli -- agent wait 1 --timeout-ms 90000` settles `done`; `fv shot composer-sent` shows the bubble and `MANGO`, sidebar row `New thread`.
- **Start (scripting).** Run `fv cli -- thread start --cwd /tmp/fv-$FV_RUN/repo --title "Probe" --focus --ready --timeout-ms 60000 --args=--model --args=haiku`. Exit 0, `thread: 1`, `attention: idle`; `fv find view-thread-1` answers bounds (the window shows it). `fv shot thread-started`: sidebar under `verify` lists `Probe`; Pane header reads `Probe` and `main`.
- **Model really pinned.** `. /tmp/fv-$FV_RUN/run.env; pgrep -P $SERVER_PID -fl claude` (Linux: `-af`) shows `--model haiku`; the Pane footer chip reads `Haiku …`.
- **Prompt from the keyboard.** With the Thread focused, `fv click --selector composer-block`, `fv type "…"`, `fv key enter`; settle with `fv cli -- agent wait <id>`.
- **Prompt (scripting).** Run `fv cli -- agent prompt 1 "Reply with exactly the word PINEAPPLE and nothing else. Use no tools." --wait --timeout-ms 90000`. Exit 0, `attention: done`, `provider_session` set.
- **Read.** Run `fv cli -- agent read 1 --lines 20`. `text` holds the prompt line and `PINEAPPLE`. `fv shot thread-replied`: the bubble and `PINEAPPLE` are visible in the Pane.
- **Rename.** Run `fv cli -- thread rename 1 "Renamed probe"`. Answer `title: "Renamed probe"`; `fv shot thread-renamed` shows the new title in the sidebar and header.
- **Park.** Run `fv cli -- thread close 1`, then `fv cli -- thread list`. List is `[]`; `fv shot thread-parked`: sidebar `Parked` shows `1` and `No open threads`; the Pane shows `What should we build in verify?`.
- **Revive.** Run `fv cli -- thread revive 1 --args=--model --args=haiku --ready --timeout-ms 60000`. Same id `1`, `attention: idle`. `fv cli -- agent read 1` still holds the PINEAPPLE exchange.

## Gotchas

- Without `--args=--model --args=haiku` the Thread runs the CLI default (Opus), whatever the scratch `settings.json` says.
- `--args --model haiku` is a command-line error (exit 2); each value needs its own `--args=`.
- A revived Thread is not shown; run `fv cli -- thread focus 1`, and check `fv find view-thread-1`, before screenshotting it.
- `focused` in `thread get`/`thread list` is `false` in every headless run: it means the operator is looking, which needs the window to have the system's focus. Read the shown Thread with `fv find view-thread-<id>`.
- A `Claude CLI … is available` toast may cover the top-right of the Pane for a few seconds after launch.
- Composer-started Threads run Opus (known model bug). Prefer `thread start --args=--model --args=haiku` and only prompt from the keyboard.
- `--wait` can fail `agent_prompt_stalled` if the agent never starts working; read the transcript before retrying.
