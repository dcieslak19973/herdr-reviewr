# Table cell wrapping: Plan

Delivers `specs/markdown.md#layout` (the table bullets).

## Problem

A table one word wider than the pane loses its grid rendering entirely and degrades to dim
wrapped source text. The user hit this previewing `specs/markdown.md`: the element table
renders as a grid at one pane width and flips to pipe soup a few cells narrower. The flip on
resize reads as a glitch, and the punishment is disproportionate to the overflow.

## Goal

An over-wide table shrinks its widest column and wraps cells inside it, staying an aligned
grid. Source-text fallback survives only for tables whose column floors still overflow.

## Definition of Done

- [x] A table one word over the pane width renders as a grid with its widest column shrunk and its cells wrapped.
- [x] A word wider than its shrunk column hard-breaks inside the column, separators aligned.
- [x] A column never shrinks below the smaller of its natural width and 8 cells.
- [x] A table over-wide at every floor renders as its dim source text, as today.
- [x] A dim rule follows the header row. No separator line divides body rows.
- [x] A link in a wrapped cell is clickable on every display row it occupies.
- [x] A table leading a list item keeps its bullet on the first rendered row.

## Out of Scope

- Alignment markers (`:---:`). Ignored today, ignored after (specs/markdown.md non-decision).
- Row separators between wrapped body rows. Rejected in brainstorming, recorded in the spec.

## Execution Plan

1. [x] `src/markdown.rs` `end_table`: after natural sizing, add the shrink loop — while the
       total overflows the budget, drop the widest column to the next-highest width or its
       floor (`min(natural, 8)`); if the floored total still overflows, keep the existing
       source-text fallback path unchanged.
2. [x] Replace the one-line row emitter: wrap each cell's chunks via `wrap_fragments`
       (word mode) at its column width, pad every cell to the row's line count, emit each
       display row with continuing ` │ ` separators, header rows bold with the dim rule
       after, `LinkSpan`s offset per display row by the cell's start column.
3. [x] Tests beside the code, one per DoD line, plus the resize boundary: the same table at
       fitting width renders identically to today's output.

## Likely Files

| file              | change                                             |
| ----------------- | -------------------------------------------------- |
| `src/markdown.rs` | shrink loop, multi-line row emission, tests        |

## Verification

- `cargo test markdown` → new table tests pass, existing table tests unchanged.
- `just ci` → clean.
- `python3 scripts/bench_tui.py --binary target/release/herdr-reviewr --fixture` A/B against
  a rebuilt baseline on a quiet system → medians unchanged (render path touched).
- Tight: everything the diff adds is exercised by a DoD line.
- Gate: promote `specs/markdown.md` to Current.

## Replan

- If GFM row normalization in pulldown-cmark does not pad short rows as assumed, then ragged
  rows need explicit cell padding in step 2.
- 2026-08-01: glow/rich research showed reference renderers level tied columns together → "Tied widest columns shrink together" added to the spec, the leveling step restored in the loop, one tie test added.
- 2026-08-01: initial plan.
