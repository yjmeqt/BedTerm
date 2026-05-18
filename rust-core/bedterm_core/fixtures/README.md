# Mock-TTY fixtures

These asciinema v2 `.cast` files drive the `replay` program for renderer
verification. `tiny.cast` is a hand-crafted sanity fixture for the parser; the
other three are real recordings of short interactive sessions.

| File              | What it exercises                                         | Source     |
|-------------------|-----------------------------------------------------------|------------|
| tiny.cast         | Sanity test for the .cast parser                          | synthetic  |
| vim-edit.cast     | alt-screen, mode-driven cursor shape, SGR truecolor       | recorded   |
| codex-tui.cast    | synchronized output (?2026), bracketed paste, truecolor   | recorded   |
| claude-code.cast  | OSC 8 hyperlinks, OSC 133 prompt markers, scrolling       | recorded   |

To re-record:

```sh
brew install asciinema
asciinema rec --cols 100 --rows 30 --idle-time-limit 1 --overwrite \
  /tmp/<name>.cast -c '<command>'
cp /tmp/<name>.cast rust-core/bedterm_core/fixtures/<name>.cast
cp /tmp/<name>.cast BedTerm/Resources/DebugFixtures/<name>.cast
./scripts/build-rust-xcframework.sh Debug   # echo_shell include_str! picks up new bytes
```

`--cols 100 --rows 30` keeps the playback geometry sensible on iOS. The
`--idle-time-limit 1` flag compresses idle pauses so replay does not stall.
