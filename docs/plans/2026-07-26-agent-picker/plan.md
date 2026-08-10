# Agent picker: Plan

Delivers `specs/herdr-host.md#agent-picker` and `specs/input.md#agent-picker`.

## Problem

`Send` refuses whenever the sidebar cannot resolve one agent, with "several agents here — copy to the clipboard instead". The reviewer's only path is then `Copy`, which writes the clipboard on the machine the binary runs on. Over SSH or mosh that is the remote machine, and a local paste inserts the local clipboard, so the comments never reach any agent. "No clipboard over SSH" is already a non-goal, so the refusal leaves a multi-agent worktree with no way to deliver a review at all.

It bites in two layouts: a sidebar opened with `tab` placement, whose own tab holds no agent, and any tab holding two agents.

## Goal

`Send` opens a picker when the workspace holds several agents, and the reviewer chooses one. One agent still sends directly. Turn tracking is untouched.

## Definition of Done

- [x] One agent sends directly, as today. No agent, and a failed enumeration, both refuse and name the clipboard.
- [x] Several agents open the picker, listing every workspace agent in herdr's own order, name bright and `state · tab` dim.
- [x] The highlight opens on the last agent this session sent to, else the pane named by `focused_pane_id`, else row 1.
- [x] `j`/`k`/arrows move, `1`–`9` jump, a click highlights, `enter` sends, `esc` cancels keeping every comment.
- [x] Every other key and gesture is inert, and none reaches the view behind. `q` does not quit. `y` does not copy. `s` does not re-export.
- [x] A successful send names the chosen agent in the status line, focuses its pane, and consumes the comments.
- [x] A chosen pane that closed while the picker was open fails the send and keeps every comment.
- [x] A refresh behind the picker adds, drops, or reorders no row, and moves no place state.
- [x] A picker taller than the pane scrolls with the highlight. Only the first nine rows carry a number.
- [x] A config error closes the picker and drops its frozen rows. Recovery lands on the view the picker opened over, with every comment intact.
- [x] Turn tracking behaves identically: tab first, workspace second, unresolved samples nothing.

## Out of Scope

- Turn tracking's own agent resolution. PR #27 owns it.
- The herdr socket transport, `events.subscribe`, and `[[link_handlers]]`. `specs/herdr-host.md` Non-goals forbid the first two today, which is a separate spec decision.
- Splitting `specs/input.md`, which is 701 words over its ceiling with 585 inherited.
- Popup geometry. Centered and content-sized is a rendering choice, deliberately absent from the specs.

## Execution Plan

1. [x] Resolve the rename unknown before building rows. Rename one agent with `herdr agent rename`, then re-read `herdr agent list`. Verified on 0.7.5: the JSON carries no `name`, `display_agent`, or `state_labels` across all 10 live agents. If a rename does not surface, the spec's disambiguation workaround is false and routes back to brainstorming.
2. [x] `src/herdr.rs`: add `name`, `display_agent`, and `state_labels` to `AgentPane`, all defaulting so 0.7.5's omissions parse. Add `workspace_agents()` returning the workspace's agents in `agent list` order. Add `tab_labels(ws)` over `herdr tab list --workspace`, joined on `tab_id`.
3. [x] `src/herdr.rs`: leave `pick_agent`, `candidates`, and `resolved_agent_status` behaviorally untouched, so tracking keeps tab-then-workspace. Delete `resolve_agent_pane`, whose only caller is the send.
4. [x] `src/export.rs`: `Agent` carries the chosen pane and its display name. `export()` sends to that pane instead of resolving. `success_message()` names the agent.
5. [x] `src/app.rs`: add `Mode::Picker` beside `List` at line 123, holding the frozen rows and the highlight. Add `open_picker`, `picker_move`, `picker_pick`, `close_picker`, modelled on `open_list`/`list_move`/`close_list`. Add the `footer_bands` arm beside `Mode::List` at line 3095. Leave the picker out of `carry_authored_state_from`'s carried arm at line 744, and close it in `set_config_error` beside the `close_search`/`close_find` pair at line 698.
6. [x] `src/app.rs`: route `export(&Agent)` through the decision — one agent sends, several open the picker, none or an error refuses. The empty-store guard at line 3272 keeps precedence.
7. [x] `src/lib.rs`: add the `Mode::Picker` key block before the `Mode::List` block at line 1443, matching only the listed actions and falling through to nothing. Add the picker to the modal mouse capture at line 1591.
8. [x] `src/ui.rs`: add `render_agent_picker`, centered by `centered` and sized to content, and `hit_picker_row` beside `hit_file` and `hit_diff`. Dim the trail with `overlay0` (`specs/theme.md`), which is what separates the two weights.
9. [x] `src/app.rs`: `reconcile_world` leaves the frozen rows and place state untouched while the picker is open, the same shape as the composing freeze.
10. [x] Tests in `tests/app_flow.rs` and `tests/render.rs`, per Verification below.
11. [x] Docs: `docs/herdr-api-notes.md` lines 75-76 are stale twice over — the pre-picker resolution rule, and the claim that reviewr lists itself as an agent. The same dead premise sits in the `src/herdr.rs:121-123` comment and its two tests. Add the `tab list --workspace` shape. Update `CHANGELOG.md`.

## Likely Files

| file                 | change                                                        |
| -------------------- | ------------------------------------------------------------- |
| `src/herdr.rs`       | agent fields, workspace candidates, tab labels                |
| `src/export.rs`      | `Agent` carries its pane and name                             |
| `src/app.rs`         | `Mode::Picker`, its verbs, footer, freeze, recovery           |
| `src/lib.rs`         | send routing, picker keys, mouse capture                      |
| `src/ui.rs`          | picker renderer and row hit test                              |
| `tests/app_flow.rs`  | picker lifecycle and send outcomes                            |
| `tests/render.rs`    | row layout, numbering, scrolling                              |

## Verification

- `just ci` → green.
- `python3 scripts/bench_tui.py --binary target/release/herdr-reviewr --fixture` → median within the `scripts/bench-results/` baseline. The picker touches no reload, render, git, or highlight hot path, so this is a sanity check.
- Live: `just qa-install`, then the user reopens a pane in `wR`, which holds two agents in two tabs. Write comments, `s`, pick, confirm the status line names the agent and its pane takes focus.
- Live: the same in a `tab`-placement sidebar, where the tab holds no agent.
- Tight: everything the diff adds is exercised by a DoD line. Delete or defer the rest.

Invariants bound to named tests:

| code                       | test                                                      | signal                                  |
| -------------------------- | --------------------------------------------------------- | --------------------------------------- |
| `HH-AGENT-PANES`           | a shell and a second reviewr pane beside one agent        | one candidate, no picker                |
| `HH-NOT-SELF`              | the sidebar's own pane id among the listed panes          | never a row                             |
| `HH-TAB-WINS`              | tracking with one tab agent and three workspace agents    | resolves the tab agent, unchanged       |
| `HH-REFUSE-SAYS-CLIPBOARD` | zero agents, and a failing `agent list`                   | both name the clipboard, no picker      |

- Gate: high-effort `/code-review` loop until clean, then `/garfield`, then promote both specs to Current, then `just qa-install`.

## Replan

- 2026-07-26: review found the footer dropped any status too long for row 1, so DoD lines 1 and 6 were unobservable in a real sidebar. `added 1 comment to claude` needed 80 columns and the refusal needed 120. Fixed in the footer, where the status now outranks the cursor's actions and truncates, not by shortening the messages alone. `specs/input.md` gained the row-1 rank the code had invented for itself.
- 2026-07-26: step 1 resolved. `herdr agent rename` does surface `name`, so the disambiguation workaround holds. herdr rejects names with spaces, so the mockup's `release bot` row was impossible and became `release-bot` in `specs/herdr-host.md`. `--clear` leaves `name` present and null, so parsing treats null as absent.
- If `agent list` order does not group by tab, rows cannot honour "tab before tab" from the CLI alone. Record what the order is and revisit the spec line.
- 2026-07-26: initial plan. Supersedes PR #36 by @shumkov, whose independent implementation is credited in the PR description.
