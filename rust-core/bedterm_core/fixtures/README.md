# Mock-TTY fixtures

These asciinema v2 `.cast` files drive the `replay` program for renderer
verification. Each is short (a few seconds) and synthetic — not literal
recordings, but hand-crafted byte streams that exercise specific terminal
features.

| File              | What it exercises                                         | Source     |
|-------------------|-----------------------------------------------------------|------------|
| tiny.cast         | Sanity test for the .cast parser                          | synthetic  |
| vim-edit.cast     | alt-screen, mode-driven cursor shape, SGR truecolor       | synthetic  |
| codex-tui.cast    | synchronized output (?2026), bracketed paste, truecolor   | synthetic  |
| claude-code.cast  | OSC 8 hyperlinks, OSC 133 prompt markers, scrolling       | synthetic  |

To re-record from a real session: install asciinema (`brew install asciinema`),
capture a short representative session with `asciinema rec`, and copy the file
into this directory plus `BedTerm/Resources/DebugFixtures/`. Keep each file
under 50 KB.
