# Files

The Files Surface in the Panel shows the Thread's folder as a tree, with each opened
file in its own tab above the editor. Its file tabs are stops in the Panel's keyboard
row with the Panel's own tabs, so the keyboard moves along them, shows one and closes
one.

## Sub-features

- `files-open` double-clicking a tree row opens that file in a tab and shows it.
- `files-tab-keys` F6 enters the Panel's keyboard row. Left, right, home and end move
  along it. Enter shows the file tab the keyboard is on.
- `files-tab-close` delete, backspace or `cmd-w` close the file tab the keyboard is on.
  The keyboard moves to the tab that takes its place, or to the Files tab once the last
  file tab closes.

## How to get to it (user POV)

- `cmd-shift-f`, or the Files tool in the title bar, opens the Files Surface.
- Double-click a file in the tree. Its tab appears above the editor.
- F6 puts the keyboard on the Panel's row, at the stop it last had, else on the shown
  Surface's tab.

## Driving it with fv

Preconditions:

- Baseline run; `fv doctor` all `ok`. Add `a.rs` (`fn main() {}`) to
  `/tmp/fv-$FV_RUN/repo` and commit it before the Thread starts. One haiku Thread
  started with `--focus`. No prompt is needed.

- **Open two tabs.** `fv key ctrl-shift-f`, then
  `fv click --selector files-row-0 --double` (`a.rs`) and
  `fv click --selector files-row-1 --double` (`README.md`). `fv find files-tab-0` and
  `files-tab-1` answer bounds. Here `a.rs` is 64 wide and `README.md` 121.
- **Which file is shown.** `files-show-source` is drawn only while a rendered file
  (Markdown or `.txt`) is shown. With `a.rs` and `README.md` it tells the two apart:
  drawn means `README.md`, `nothing drawn` means `a.rs`. `files-prose` cannot tell
  them apart if both files are `.txt`, because `.txt` renders as prose too.
- **The row's order.** It is the order the Panel draws: the Panel's Surface tabs
  (`Overview`, `Files`, …), then `+`, then maximize, then the file tabs, left to
  right. With Overview and Files open and two file tabs, the row is
  `Overview, Files, +, maximize, file 0, file 1`. Home goes to `Overview`, not to a
  file tab, and end goes to the last file tab.
- **Show with Enter.** `fv key f6 end left enter`: the keyboard enters at the Files
  tab, end goes to `README.md`, left to `a.rs`, and enter shows it
  (`files-show-source` is no longer drawn). Then `fv key right enter`:
  `files-show-source` is drawn again.
- **Close with the keyboard.** `fv key left delete`: `files-tab-1` is gone and the one
  tab left is 121 wide, `README.md`. `backspace` does the same. `ctrl-w` (`cmd-w` on
  macOS) closes the file tab the keyboard is on, not the Thread.
- **Delete on a Panel tab closes the Surface.** `f6 home right delete` with a fresh
  row lands on `Files` (home is `Overview`, right is `Files`) and closes the whole
  Files Surface. Delete on `+` or maximize does nothing. To reach a file tab from
  home, press right past `Files`, `+` and maximize.

## Gotchas

- Linux has no screenshots, and nothing in the window reads out which stop holds the
  keyboard. Prove each step by what it changes: which file shows
  (`files-show-source`), which tabs are drawn, and their widths.
- A pointer press on a file tab shows that file, but the keyboard stays where it was,
  as it does for every Panel stop. Keys pressed after the click do not reach the tab,
  and `fv key delete` answers `unhandled: ["delete"]`. Use F6 to bring the keyboard to
  the row.
- F6 returns to the stop the keyboard last had in that Thread's Panel, so after a
  close it lands on the tab that took the closed one's place, not on the Files tab.
- Tree rows are sorted without regard to case: `a.rs` comes before `README.md`.
