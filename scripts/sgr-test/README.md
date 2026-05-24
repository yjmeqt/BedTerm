# SGR rendering test scripts

Visual checks for SGR 1 (bold), 3 (italic), 4 (underline), 7 (inverse)
in BedTerm. Run remotely via SSH and eyeball the output.

| Script | Purpose |
|---|---|
| `sgr-basic.sh` | One row per attribute; colour×style matrix; reset-attr round-trip |
| `sgr-realworld.sh` | Mimics `ls --color`, `git status`, `grep`, `man`, `diff` |
| `sgr-cjk.sh` | Bold / italic / underline / inverse over CJK strings — exercises the fallback cascade |
| `sgr-symbols.sh` | Block elements, geometric shapes, misc technical, dingbats — exercises font-fallback for non-ASCII symbols |

## What "good" looks like

- **bold**: noticeably heavier strokes than regular on the same row. If
  the bold face didn't load, this row looks identical to regular.
- **italic**: slanted glyphs. Apple's Menlo-Italic is a true italic
  (not faux-oblique), so it should look distinctly cursive.
- **underline**: a hairline runs the full cell width directly below
  the baseline. Coloured rows should keep their foreground colour on
  the underline.
- **inverse**: foreground and background swap. On a black surface,
  a default-fg cell shows white block with black text.
- **reset (SGR 22 / 23 / 24 / 27)**: the middle "off" segment turns
  the attribute off without losing colour.

## Failure modes to watch for

- Bold and regular look identical → Menlo-Bold not loaded (check
  startup log for `[BedTerm] terminal font: Menlo`).
- Underline missing on every row → SGR 4 not yet implemented.
- Inverse just changes text colour but bg stays default → SGR 7 not
  yet implemented.
- CJK rows show tofu (▢▢▢) under bold → bold attribute is preventing
  the CJK fallback from matching; needs investigation.
- Block elements show gaps or overlaps between adjacent cells → cell
  metrics don't match the rasterized glyph width; the glyph may be
  getting centered with too much padding.
- ⏵ (U+23F5), ⏺ (U+23FA), ⎿ (U+23BF) show tofu → fontdb has no
  fallback face covering Misc Technical. On macOS, STIXTwoMath /
  Hiragino Sans supply these; on iOS the sandbox hides system fonts
  from fontdb. Fix is to bundle a symbol fallback (e.g. Noto Sans
  Symbols) covering U+2300–U+23FF.
- Dim bar looks identical to solid bar → SGR 2 (faint) not yet
  implemented in the renderer.
- Inverse + colour rows show wrong bg fill → SGR 7 palette swap not
  applied to the cell's background attribute.
