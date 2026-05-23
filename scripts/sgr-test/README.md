# SGR rendering test scripts

Visual checks for SGR 1 (bold), 3 (italic), 4 (underline), 7 (inverse)
in BedTerm. Run remotely via SSH and eyeball the output.

| Script | Purpose |
|---|---|
| `sgr-basic.sh` | One row per attribute; colour×style matrix; reset-attr round-trip |
| `sgr-realworld.sh` | Mimics `ls --color`, `git status`, `grep`, `man`, `diff` |
| `sgr-cjk.sh` | Bold / italic / underline / inverse over CJK strings — exercises the fallback cascade |

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
