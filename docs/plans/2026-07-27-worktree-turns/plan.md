# Worktree turns: Plan

Delivers `specs/herdr-host.md#turn-tracking`, `specs/review-model.md#turn-baseline`, and `specs/tui.md#refresh`.

## Problem

A reviewer with more than one agent, or any sidebar placement other than `split`, gets silently degraded review. `last-turn` stalls forever on "waiting for the agent's next turn", and the PR tab quietly loses its per-turn refresh along with it. The cause is that reviewr re-infers "the agent" every poll from pane topology, and topology is not what the reviewer is reviewing.

## Goal

Turn tracking follows the worktree instead of one inferred agent, so `last-turn` behaves identically at any agent count and any sidebar placement.

## Definition of Done

- [ ] Two agents working in one worktree produce one turn. Neither one's start re-baselines the other's in-flight work.
- [ ] With `toggle_placement = "tab"`, turns track exactly as under `split`.
- [ ] An agent in a sibling worktree of the same repository never affects this sidebar's turns.
- [ ] A worktree that held no agents starts a turn when the first agent arrives and works.
- [ ] `Changes` under `last-turn` reads `no agent works here` when the worktree holds no agents.
- [ ] `Changes` under `last-turn` reads `waiting for the first turn` when agents are present and no turn has been observed.
- [ ] A failed `agent list` leaves the previous membership and message unchanged.
- [ ] A turn end fires the ambient PR refetch at any agent count.
- [ ] Send and the agent picker behave exactly as before, workspace-scoped.

## Out of Scope

- Per-agent attribution inside a `last-turn` diff. herdr reports no per-file authorship (`specs/herdr-host.md` Non-goals).
- The agent picker's rows and arming ladder. Unchanged by this work.
- One baseline shared by two sidebars on one worktree. They now agree on which turn is running, but each still snapshots on its own poll clock and shows its own baseline. The bound is stated in `specs/herdr-host.md` Failure semantics.
- Per-agent turn identity. An agent left at a permission prompt holds the whole worktree at neither, so no other agent in it starts or ends a turn until the prompt is answered. Releasing that needs to know which agent was working, which is the identity `HH-TURN-PER-WORKTREE` trades away. Accepted: the failure direction is a diff that shows too much, never too little, and it clears itself. The bound is stated in `specs/herdr-host.md` Failure semantics.

## Execution Plan

1. [ ] Parse `cwd` on `AgentPane` (`src/herdr.rs`). Add `cwd` to the `TWO_AGENTS` and `ONE_AGENT` fixtures in `tests/send_flow.rs` so the existing send tests keep passing against the richer shape.
2. [ ] Add `WorktreeState { Resting, Working, Neither }` to `src/turn.rs`, and fold a member set's `Status` values into it. An empty member set is `Resting`.
3. [ ] Replace `pick_agent` with a worktree member query in `src/herdr.rs`, resolving each agent's `cwd` through `git::toplevel` and comparing against the sidebar's repo. Memoize `cwd` to top level in a `HashMap`, since a directory's top level does not change and the poll repeats every 2 seconds.
4. [ ] Replace `resolved_agent_status` with a function returning the aggregate `WorktreeState` and whether any agent is present. `Err` still means the enumeration failed, and the caller holds its previous value.
5. [ ] Change `TurnTracker::observe` in `src/turn.rs` to take `WorktreeState`. Rewrite the five edge tests from `a_turn_starts_when_working_follows_a_resting_status` to `a_lone_first_working_sample_never_starts_a_turn` as aggregate edges. `a_question_only_turn_keeps_the_previous_baseline` drives `observe` too and follows them. `from_wire` and the two promote tests are unaffected.
6. [ ] Carry agent presence on `TurnReport` (`src/world.rs`), updated only by a successful enumeration. Mirror it onto `App` beside `turn_baseline`, the way `sync_turn_baseline` already does.
7. [ ] Split `App::awaiting_turn` (`src/app.rs:1214`) into the two empty states, and paint them at the two `src/ui.rs` sites (lines 601 and 878).
8. [ ] Add a tracking test to `tests/send_flow.rs`, driving the fake herdr with two agents in one worktree, one in a sibling worktree, and a failing `agent list`.
9. [ ] Run the bench against a pre-change binary built to a second target dir, interleaved, and compare medians.

## Likely Files

| file                 | change                                                                                 |
| -------------------- | -------------------------------------------------------------------------------------- |
| `src/herdr.rs`       | parse `cwd`, member query with a top-level cache, aggregate state, drop the tab ladder |
| `src/turn.rs`        | `WorktreeState`, `observe` takes the aggregate, edge tests rewritten                   |
| `src/world.rs`       | `TurnHost::sample` reads the aggregate, `TurnReport` carries presence                  |
| `src/app.rs`         | presence mirror, `awaiting_turn` splits into two states                                |
| `src/ui.rs`          | two empty-state strings at lines 601 and 878                                           |
| `tests/send_flow.rs` | `cwd` in the fixtures, new worktree-tracking tests                                     |

## Verification

- `just ci` → green.
- `cargo test --test send_flow` → the new tracking tests pass, and the existing send tests are untouched by the fixture change.
- `HH-TURN-PER-WORKTREE` → `two_agents_in_one_worktree_produce_one_turn` → one turn start, and the second agent's start captures no candidate.
- `python3 scripts/bench_tui.py --binary target/release/herdr-reviewr --fixture` before and after, interleaved under the same load → medians within noise. The member query runs on the world worker, so a regression here would mean the cache is not holding.
- Tight: everything the diff adds is exercised by a DoD line. Delete or defer the rest.
- Gate: promote `herdr-host.md`, `review-model.md`, and `tui.md` to Current.

## Replan

- If `git::toplevel` proves too slow even memoized, then compare canonicalized path prefixes instead and accept that a nested repository inside the worktree counts as a member.
- 2026-07-27: initial plan.
- 2026-07-27: review found that a parked agent holds the worktree at neither indefinitely → weighed per-agent hold identity and a rolling all-quiet anchor against accepting the bound, accepted it → new Out of Scope entry, `specs/herdr-host.md` Failure semantics.
- 2026-07-27: review found `TurnHost` and `App` derive the baseline-ref key from the repo path independently, and the blocked-config path fed the raw one → normalize once in `run` (`repo_root`) rather than inside `TurnHost::open`, which would key the two apart → `src/lib.rs`, `tests/app_flow.rs` `turn_setup`.
- 2026-07-27: review + distill found membership collapsed "git said not a member" and "git could not run" into one `bool`, so a transient cwd-resolution failure under load folded an empty membership to `Resting`, ended a live turn, misfired the PR refetch, and painted `no agent works here` while an agent worked → weighed folding the resolved subset against holding the poll, chose to hold on an undetermined member exactly as on a failed enumeration → gave membership a third state (`Unknown`) via `git::worktree_of` (`src/git.rs`), routed it through a pure `classify` and cached both determinations so a non-member stops re-shelling (`src/world.rs`), `specs/herdr-host.md` Turn tracking. Also collapsed `TurnTracker::prev` to the one bit it read and factored the shared agent-pane predicate (`src/turn.rs`, `src/herdr.rs`).
- 2026-07-27: re-review found the first cut cached the `Outside` verdict, so a transient nonzero git exit (not just a spawn failure) could poison a member for the session, and the repo-root fast path went untested because macOS canonicalizes the temp path → cache only the stable positive fact (`resolved: HashMap<String, bool>`, members only), leave `Outside`/`Unknown` uncached and self-healing, and drop the fast path since git canonicalization on the general path subsumes it → `src/world.rs`, plus `worktree_of` and mixed-membership unit tests.
