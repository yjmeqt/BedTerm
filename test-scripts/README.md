# BedTerm Manual Test Scripts

Pure bash; no external deps. Run from `test-scripts/` after connecting the sim
to your local shell. Each script tests one concrete BedTerm behavior — when
something visibly breaks, the script number tells you which area to look at.

| #  | Script                       | What it exercises                                                        |
|----|------------------------------|--------------------------------------------------------------------------|
| 01 | `01-scroll.sh`               | BlockGrid scrollback (seq 200) — body must scroll up past viewport       |
| 02 | `02-cjk.sh`                  | Wide chars + emoji column count                                          |
| 03 | `03-stdin-read.sh`           | `read -p` while running — composer passthrough must deliver stdin        |
| 04 | `04-stdin-cat.sh`            | `cat` echo loop + Ctrl-D EOF                                             |
| 05 | `05-progress.sh`             | `\r` cursor carriage return doesn't leak between blocks                  |
| 06 | `06-mock-vim.sh`             | Mock alt-screen TUI: enter/draw/leave — verifies mode switch + restore   |
| 07 | `07-mock-claude.sh`          | Mock TUI with box chrome + streaming reply — passthrough collapse        |
| 08 | `08-mock-sleep-task.sh`      | Long-running task with timer + p/r/q controls via passthrough stdin      |
| 09 | `09-color.sh`                | 16 / 256 / truecolor palette resolution                                  |
| 10 | `10-bell-clear.sh`           | BEL doesn't crash, `clear` only clears current block                     |
| 11 | `11-signals.sh`              | Ctrl-C latch via composer's Ctrl chip + trap behavior                    |
| 12 | `12-resize.sh`               | `$COLUMNS` / `$LINES` and SIGWINCH react to keyboard show/hide           |

## Running

```sh
cd ~/Developer/BedTerm/.worktrees/block-view/test-scripts
./06-mock-vim.sh   # press q to leave
./07-mock-claude.sh   # type messages, /quit to leave
./08-mock-sleep-task.sh   # p=pause r=resume q=quit
```

## Inspection points

After each script ends, check the block header:
- exit code (0 expected unless documented)
- duration (matches the script's own wall clock)
- body extent fits the output without truncation / overflow

For alt-screen scripts (06, 07): the host UI must flip to the classic full
grid while running and flip back to the block list when the script exits.
