# Mock-TTY fixtures

These asciinema cast files drive the `replay` program. They are embedded
into the `bedterm_core` rlib at compile time via `include_str!` and looked
up by `replay::preset_cast(name)`. The iOS app references them by name
through the `"preset": "<name>"` option on `bt_mock_tty_create`, so there
is **no second copy** in `BedTerm/Resources/`.

| File              | Preset name(s)         | What it exercises                                       | Source     |
|-------------------|------------------------|---------------------------------------------------------|------------|
| tiny.cast         | (internal, parser test)| Sanity test for the .cast parser                        | synthetic  |
| vim-edit.cast     | `vim`, `vim-edit`      | alt-screen, mode-driven cursor shape, SGR truecolor     | recorded   |
| codex-tui.cast    | `codex`, `codex-tui`   | synchronized output (?2026), bracketed paste, truecolor | recorded   |
| claude-code.cast  | `claude`, `claude-code`| OSC 8 hyperlinks, OSC 133 prompt markers, scrolling     | recorded   |

The parser handles both asciinema v2 (absolute times) and v3 (per-event
deltas); the version is auto-detected from the `"version"` field in the
header line.

To re-record:

```sh
brew install asciinema
asciinema rec --idle-time-limit 1 --overwrite \
  rust-core/bedterm_core/fixtures/<name>.cast -c '<command>'
./scripts/build-rust-xcframework.sh Debug   # picks up the new include_str! bytes
```

The host TTY geometry at recording time becomes the cast's `cols × rows`.
asciinema 3.x ignores `--cols / --rows`; set the host terminal size with
`stty cols 100 rows 30` first if you need a specific geometry.
