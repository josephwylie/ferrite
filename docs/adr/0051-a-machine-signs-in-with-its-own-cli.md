---
status: accepted
date: 2026-11-26
---

# A Machine signs in with its own CLI

A Thread on another Machine fails when that Machine's Claude or Codex is
not signed in, and until now the operator had to reach the Machine in a
terminal and run the CLI's login there. Now the window offers it in one
press: `Sign in to Claude on josephpc`.

## Decision

The Machine's Server runs its own CLI's sign-in, and the window walks it a
step at a time with the `sign_in {provider, step}` command (ADR 0025), each
step answered once:

- `status`: whether the CLI is signed in there (`sign_in_status`; no
  answer for a CLI that is missing or says nothing).
- `start`: ends a sign-in of that provider already running there, starts
  the CLI's own (`claude auth login`, `codex login --device-auth`), and
  answers with the page it names (`sign_in_page`).
- `finish {code?}`: hands Claude the code its page showed, or waits on
  Codex's page being approved, then asks the CLI's status (`signed_in`).
  Claude refuses a code cut short on sight and reads another, so a CLI
  that answers the code with a line and then runs on, quiet for 3 s,
  answers `code_refused` with that line; the sign-in keeps running for
  the code pasted again.
- `cancel`: stops the CLI and everything it started (`changed`).

The page opens in this computer's browser. The Machine ends up with a
sign-in of its own, made by its own CLI. Nothing is copied between
computers. Once signed in, the Server ends that provider's stale Sessions
and finds its models again.

A sign-in belongs to the window that started it. One that window cancels,
or that goes with the window, is stopped with its whole process tree: a
process group on Unix and a Job on Windows. Codes travel as `Withheld`,
whose `Debug` shows nothing. Nothing logs a page, a code or what a CLI
wrote.

The window offers the sign-in in two places, both only for a Machine
other than this computer, whose CLI opens its own browser:

- The failure strip of a Thread whose failure reads as signed out
  (`ferrite_core::sign_in::signed_out`), beside `Retry`.
- Settings › About › Provider CLIs, under a CLI that says it is not
  signed in there.

Both open one sheet (`ferrite_settings::sign_in::SignInView`). A Server
from before this answers `unsupported`, and the sheet says to update
Ferrite there.

## The two shapes

A. The Server owns the running sign-in, as above.

B. The window runs `ssh <target> claude auth login` itself.

A is taken. In B the process would have two owners, the window and the
Machine. Without a PTY, ssh going away would not end the CLI, which waits
on its stdin through EOF. And B would skip what ferrited already gives a
CLI: its login environment, its discovery of the CLI and the links that
reach a Machine without ssh.

## Codex signs in by device code

Plain `codex login` waits for its page to call back to
`localhost:1455`, on the Machine. A browser on this computer cannot reach
that. `codex login --device-auth` instead shows a code to enter on its
page and waits for the page's approval, needing nothing on its stdin.

The account must allow device code sign-in, which ChatGPT's security
settings hold. Where it does not, the page refuses the code and the sheet
says to turn it on. Otherwise the operator signs in on the Machine itself,
as before.

## What is not proven

The tests run scripts standing in for both CLIs. A live run took the real
`claude` 2.1.332 and `codex` 0.171.0 on a WSL Machine, from a Windows
window, as far as their pages: each page read whole, Claude's refusal of a
code cut short, its refusal of a wrong whole code, and a cancel leaving no
CLI process there. Still to be shown:

- A real sign-in through a real browser, for either provider: no account
  was signed in, so the signed-in end ran a stand-in `claude`.
- That a Session started after a sign-in runs signed in.
- That Claude, once it has the right code, ends within 3 s of the last
  line it writes, as the refusal rule takes it to.
