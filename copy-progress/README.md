# Copy progress: live checks

An isolated Ferrite built from the PR head (fv, HOME and USERPROFILE on a
temporary folder) on the WSL test Machine of `scripts/wsl-machine.ps1`,
added through Settings › Connections › Add › SSH. The Project, `atlas`, was
generated for the run and deleted after: 18,000 small sources, 26 files of
90 MiB, and 554 MB that git ignores in six entries. `data/state.db` was held
open with no sharing for the whole copy, so it could not be read.

| State | Shot |
| --- | --- |
| Choose, the browser closed | `01-choose.png` |
| Choose, after Change… | `02-choose-browsing.png` |
| Some ticked (the header's dash), Copy 2.6 GB | `03-some-ticked.png` |
| All ticked, Copy 3.1 GB | `04-all-ticked.png` |
| Listing: the bar with no end, "Listing files… 5,000" | `05-listing-b.png` |
| Copying: 114 MB of 2.5 GB, 13 MB/s · about 3 min left, the file | `06-copying.png` |
| Continue in background: the toast and the chip's ring, 13% | `07-toast-and-chip.png` |
| Show on the toast: the card again, the toast gone | `08-reopened-from-toast.png` |
| The Machine's Server killed under the copy, the card closed: the plain reason, Show and Resume | `09-failed-toast.png` |
| Resume from the toast, finished with the card closed: 1 item not copied, Show | `10-finished-toast.png` |
| Show: the card on what it did, `data/state.db · in use here` | `11-finished-card.png` |
| What landed on the Machine: 2.4 GB, `.env` (ticked) there, `dist/` (not ticked) and `state.db` not | `12-atlas-on-wsl.txt` |

The resize, at `FERRITE_MOTION_SCALE=40` so a 200 ms tween lasts 8 s
(`burst/`): the Choose card at 600 wide, then each shot about a second apart
as it shrinks to 512 and grows as the numbers row comes in. `placement-card`
read back 558, 538, 526, 520, 516, 514, 514, 512 wide and 203, 195, 191,
201, 207, 211, 213, 215 tall.
