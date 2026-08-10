# Navigator hide: Plan

Delivers the hidden-navigator contract in `specs/tui.md`, the `navigator-hide` action in `specs/input.md`, and the recovery line in `specs/config.md`.

## Problem

Reviewing a wide diff spends 32% of the body on the files panel. The hunk and file steps (`]` `[` `f` `F`) already traverse the whole changeset from the diff, so the panel is often idle while it costs width.

## Goal

One key hides the navigator and brings it back, so the read pane can take the whole body.

## Definition of Done

- [x] `z` hides the navigator on the file tabs, and the read pane takes the whole body.
- [x] `z` again shows the navigator in its kept position and share, focus staying on the read pane.
- [x] While hidden, `p`, `<`, and `>` are inert, and a click where the divider sat starts no drag.
- [x] While hidden in `Normal` mode, `tab` shows the navigator and focuses it. The search screen's `tab` keeps flipping `Files`/`Code`.
- [x] While hidden, focus is the read pane, and an empty read pane's row 1 offers `z show` and `tab files`.
- [x] `PR` keeps its navigator: `z` is inert there, the footer never lists it, `p` still works, and a `PR` visit leaves the hidden state untouched.
- [x] `z` is text in the comment editor and the search input, and inert in the comments list and the agent picker.
- [x] Visible, `z hide` sits in the `?` expansion's `go` band, joining row 1 only while the file list holds focus. Hidden, `z show` joins row 1's actions, the band omits it, and `p position` drops.
- [x] A keypress mid-drag cancels the drag, keeps the last painted share, and still performs its action.
- [x] The footer keeps `tab files` while hidden, an empty changeset included.
- [x] The lone pane fills its cursor row with `surface2` while hidden.
- [x] A restart shows the navigator. Config recovery preserves the hidden state.
- [x] `navigator-hide` rebinds through `[keybindings]` like any action.

## Out of Scope

- A `navigator_hidden` config key. A `specs/tui.md` Non-goal.
- Per-tab hidden state. Covered by the one-state decision in `specs/tui.md`.
- The `config.md` end-to-end rewrite and the stale-pointer fixes in the same worktree. A separate maintenance change, landing on its own.

## Execution Plan

1. [x] `src/keymap.rs`: add `Action::NavigatorHide`, table row `("navigator-hide", z)`, and a default-binding unit test beside the `p` one (line 247).
2. [x] `src/app.rs`: add a `navigator_hidden` field beside the shares (~line 473), a toggle method that moves focus to the read pane on hide, and inertness gates in `cycle_navigator_position` and `resize_navigator`.
3. [x] `src/app.rs`: `toggle_focus` (line 1993) shows the navigator and focuses it while hidden, `Normal` mode only. The empty-read-pane focus fallback (line 1783) keeps focus on the read pane while hidden.
4. [x] `src/lib.rs`: dispatch `NavigatorHide` at both sites (lines 1519, 1572), mirroring `NavigatorPosition`'s mode gating.
5. [x] `src/ui.rs`: `split_body` (line 145, called at 128) gives the whole body to the read pane while hidden, and `hit_divider` (lines 176–189, used at `src/lib.rs:1725`) reports no divider — including the seam cell at the body's edge. Verify the keypress-cancel path keeps the last painted share while the key still dispatches, and align it if not.
6. [x] `src/app.rs`: footer — add `FooterAction::NavigatorHide`, push it as `Go` while visible and `Do` (row 1) while hidden in the file-tab `footer_bands` tail, lead with it on a hidden empty read pane, drop `NavigatorPosition` while hidden, keep `TogglePane` while hidden despite the empty-changeset gate, leave the `PR` branch without it, and flip the hint label `hide`/`show` in the `src/ui.rs` map.
7. [x] `src/app.rs`: carry `navigator_hidden` in `carry_authored_state_from`, beside the shares (lines 783–784), with a recovery test beside `config_recovery_keeps_both_shares_and_reapplies_the_configured_position` (line 3789).
8. [x] Tests in `tests/app_flow.rs`, `tests/render.rs`, and the `src/app.rs` test module: toggle round-trip keeping position and share, `tab` un-hides and focuses in `Normal` mode only, inert `p`/`<`/`>`, `z` as text in the composer and the search input, `z` inert in the comments list and picker, `z` inert on `PR` with the state untouched across a `PR` visit, row-1 `z show` on a hidden empty read pane, edge-cell click starts no drag while hidden, full-width frame with a `surface2` cursor fill, `footer_bands` in both states (`Go` visible, `Do` hidden, no `z` on `PR`), `tab files` kept with an empty changeset, keypress mid-drag cancels and keeps the share while the key acts.

## Likely Files

| file                 | change                                                       |
| -------------------- | ------------------------------------------------------------ |
| `src/keymap.rs`      | `NavigatorHide` action, `z` default, name string             |
| `src/app.rs`         | hidden field, toggle, focus and inertness gates, `footer_bands`, recovery |
| `src/lib.rs`         | dispatch at both action sites, divider hit gate              |
| `src/ui.rs`          | full-body split, `hit_divider` gate, hint label flip         |
| `tests/app_flow.rs`  | toggle, focus, inertness, footer bands, recovery flows       |
| `tests/render.rs`    | full-width frame, cursor fill, footer in both states         |

## Verification

- `just ci` → clean.
- `cargo test navigator` → the new flows pass.
- `python3 scripts/bench_tui.py --binary target/release/herdr-reviewr --fixture` A/B against a rebuilt pre-change binary on a quiet system → `tab_enter` medians unchanged (the split is on the render path).
- Tight: everything the diff adds is exercised by a DoD line. Delete or defer the rest.
- Continuity (`specs/overview.md`): hidden is place state — the mid-refresh toggle test asserts a landing world result never flips it.
- Gate: `/code-review` loop until clean, then `/garfield` end to end. Promote `specs/tui.md`, `specs/input.md`, `specs/config.md`, `specs/theme.md`, `specs/search.md`, `specs/pr-tab.md` to Current before landing.

## Replan

- If `tab_enter` medians regress, then move the hidden branch out of the per-frame geometry helpers.
- 2026-07-31: user refined the footer again → `z hide` joins the file list's calm row 1 while visible → the specs and this file.
- 2026-07-31: user overturned two locks → `PR` exempt from the hidden state, and `z show` promoted to row 1 while hidden with `z hide` kept under `?` → the specs and this file.
- 2026-07-31: re-walk → the empty-read-pane special cases deleted for an always-available toggle → the specs and this file.
- 2026-07-31: spec and plan walks → eight spec holes fixed (`PR` note, empty-read-pane rules, `tab` scoped to `Normal`, footer band and label, `theme.md` fill, drag cancel) and the plan re-grounded to `footer_bands` and `hit_divider` → the specs and this file.
- 2026-07-31: initial plan.
