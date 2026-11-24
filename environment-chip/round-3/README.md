# PR #496, round 3: live checks on the rebased head

An isolated Ferrite, its HOME and USERPROFILE on a temporary folder, on the
WSL test Machine of `scripts/wsl-machine.ps1` (its own HOME there), added
through Settings › Connections › Add › SSH.

| What | Proof |
| --- | --- |
| Connections row after the #494 rebase: Sync my setup and the projects folder field, no overlap | `01-connections-row.png` |
| A folder git ignores whole is one row; the folders take the card's free height | `02-sheet-opens.png` |
| Failed: a plain line under `~/` (`~/locked` is mode 555 there) | `03-failed.png` |
| Item 3: `keep.txt` (`-text`, CRLF here) arrives CRLF and `git status` there lists it modified, as here; `auto.txt` (no attribute, `core.autocrlf=true`) arrives LF and clean | `04-eol-app-on-wsl.txt` |
| Item 4: `build/` ticked arrives with `out.txt` and `deep/more.txt` | `04-eol-app-on-wsl.txt` |
| Copying: a first copy's button stays Copy | `05-copying.png` |
| Cancelled: the folder the copy made is listed (fixed this round, `38a54aa7`) | `06-cancelled.png` |
| Unfinished: reopens under `~/`, "An unfinished copy of resume-app is here.", Resume, no "Use what is there" | `07-unfinished.png` |
| Item 2: `src/main.rs` changed on the Machine before Resume stays "their main" and is listed; `theirs.txt` stays; the 944 MB file's hash matches | `08-resumed.png`, `09-resume-app-on-wsl.txt` |
| Item 5: a file of this computer attached on the Machine; the Machine turned off (draft put away) and on; the draft back with its file and no second upload; Local picked; sent from Local with this computer's copy | `10-attached-on-machine.png`, `11-machine-off.png`, `12-draft-back.png`, `13-draft-on-local.png`, `14-sent-from-local.png`, `15-sent-prompt.txt` |

The sent Thread fails with "Not logged in": the isolated HOME holds no sign-in.
Its prompt names `fv-rebase496\attachments\…\attach-note.txt`, the file here.
