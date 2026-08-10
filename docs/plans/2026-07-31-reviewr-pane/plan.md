# Launcher-blind reviewr pane: Plan

Delivers `specs/herdr-host.md` (Overview, Pane identity, agent picker) and `specs/config.md` (config directory resolution), closing issue #20.

## Problem

A layout plugin (herdr-plus, issue #20) cannot place reviewr: the `open` action acts on the focused workspace, runs out-of-band, and always creates its own pane. Running the binary directly in a layout pane almost works today, but the pane ignores the user's config (no `HERDR_PLUGIN_CONFIG_DIR`), is invisible to the toggle (no `reviewr` label), and has no stable path to invoke (the plugin root is hash-suffixed).

## Goal

Any pane that runs the binary is a full reviewr pane: `command = "herdr-reviewr"` in a layout file delivers the configured theme, agent send, turn tracking, and toggle interop.

## Definition of Done

- [x] With `HERDR_PLUGIN_CONFIG_DIR` unset and herdr present, the binary reads the config that `herdr plugin config-dir persiyanov.reviewr` names. Without herdr it uses defaults.
- [x] The actions identify reviewr panes by foreground process (`herdr pane process-info`), never by label. Flag runs never count.
- [x] The binary stamps its unlabeled pane `reviewr` at startup and clears only its own label on a normal quit, cosmetic only.
- [x] `toggle`/`open`/`close` treat a hand-launched or layout-launched pane exactly like one they opened.
- [x] The picker highlight arms on the last-sent agent, else row one. `opened_beside` and its env read are gone.
- [x] `herdr plugin install` links the binary at `~/.local/state/herdr/plugins/persiyanov.reviewr/bin/herdr-reviewr`, and at `~/.local/bin/herdr-reviewr` when that directory exists. Both are symlinks into the plugin root, so an uninstalled plugin fails loudly.
- [x] No `sidebar` remains in code, manifest, scripts, README, or AGENTS.md. Action ids `toggle`/`open`/`close` are unchanged.
- [x] `docs/herdr-api-notes.md` records the surface verified 2026-07-31: plain-pane env (`HERDR_PANE_ID`, `HERDR_WORKSPACE_ID`), `pane process-info` (foreground `name`/`argv`), `pane rename` (incl. `--clear`), `pane split`/`run`/`current`, `plugin config-dir`.
- [x] README's layout section shows the `command = "herdr-reviewr"` recipe with the stable path as fallback.
- [x] Release published, #20 answered with the recipe and closed.

## Out of Scope

- Folding `herdr/pane.sh` into binary subcommands, and moving the toggle off `plugin pane open` onto plain `pane split`. Follow-up cleanup, spec-silent either way.
- A `herdr plugin run <id>` launcher. Upstream nicety, file with herdr.

## Execution Plan

1. [x] Config resolution (`src/config.rs`): resolve the config directory once at startup — `HERDR_PLUGIN_CONFIG_DIR`, else `herdr plugin config-dir persiyanov.reviewr` via `src/herdr.rs`, else none — and route `plugin_config()` through it. Unit tests drive the resolver with an injected lookup; a failed or absent CLI resolves to none.
2. [x] Pane label (`src/herdr.rs` + startup/shutdown in `src/lib.rs`): `pane rename $HERDR_PANE_ID reviewr` at startup, `pane rename --clear` on normal exit, both skipped without `HERDR_PANE_ID`, failures logged only, cosmetic per `specs/herdr-host.md` Pane identity.
3. [x] Picker arming (`src/app.rs:3355-3385`, `src/herdr.rs:188-201`): delete `opened_beside()` and `PluginContext`, drop the `beside` parameter from `open_picker`/`armed_row`. Update `tests/app_flow.rs:4740` and `tests/send_flow.rs:104-196` to the two-rung ladder.
4. [x] Installer links (`herdr/install.sh`): after installing into `bin/`, `ln -sf` the two stable paths, `~/.local/bin` only when the directory exists.
5. [x] Actions rework and rename sweep: `herdr/sidebar.sh` → `herdr/pane.sh`, matching panes by `pane list` plus `pane process-info` per pane instead of by label; manifest entrypoint id `sidebar` → `pane` plus action titles ("reviewr: toggle pane"); `sidebar` identifiers and comments in `src/`, `justfile:27`, `docs/qa-install.md`, `AGENTS.md`, README.
6. [x] Docs: refresh `docs/herdr-api-notes.md`, rewrite README's "Auto-open and layout plugins" section around the recipe.
7. [x] Merge gate, then release per `docs/RELEASING.md` (minor bump: new user-facing capability), then reply on #20.

## Likely Files

| file                      | change                                                        |
| ------------------------- | ------------------------------------------------------------- |
| `src/config.rs`           | config-directory resolver, once at startup                    |
| `src/herdr.rs`            | `config-dir` call, label write/clear, delete `opened_beside`  |
| `src/lib.rs`              | label at startup, clear on the normal exit path               |
| `src/app.rs`              | two-rung `armed_row`, `open_picker` signature                 |
| `tests/app_flow.rs`       | arming ladder cases                                           |
| `tests/send_flow.rs`      | picker highlight cases                                        |
| `herdr/install.sh`        | the two stable symlinks                                       |
| `herdr/sidebar.sh`        | rename to `herdr/pane.sh`, process-identity matching          |
| `herdr-plugin.toml`       | entrypoint id, action titles, script path                     |
| `README.md`               | layout recipe, rename                                         |
| `docs/herdr-api-notes.md` | verified herdr surface, 2026-07-31                            |

## Verification

- `just ci` → clean.
- `cargo test --test app_flow` and `--test send_flow` → arming and send cases green.
- `HH-LAUNCHER-BLIND` → the arming tests run with no plugin env set → same highlight either way.
- `CFG-ONE-SNAPSHOT` → existing config tests unchanged and green.
- No bench run: startup-only additions, no reload/render/git/highlight path touched.
- Live QA: `just qa-install`, then the user reopens panes and runs one layout/hand-launched pane — theme applies, toggle closes it, quit unlabels it, and a pane whose binary exited is ignored by the next toggle. Pane opens are the user's keystrokes, never scripted.
- After the release's real `herdr plugin install`: `ls -l` both stable paths and confirm each is a symlink into the plugin's `bin/herdr-reviewr` — `install.sh` warns rather than fails on a link error, so only this check proves the paths exist.
- Gate: promote `herdr-host.md`, `config.md` to Current; the four rename-touched specs stay Current.

## Replan

- If herdr 0.7.5 lacks `pane process-info`, `pane rename --clear`, or `plugin config-dir` (verified live only on the user's current herdr), then gate each call on failure-as-no-op and note the floor in `docs/herdr-api-notes.md` — except `process-info`, which the actions cannot work without: that one bumps `min_herdr_version`.
- If the manifest entrypoint id rename breaks live pane reopen on installed plugins, then keep id `sidebar` and log one line here.
- 2026-07-31: review round → a close racing a pane's exit must converge, and a process read failing for any reason other than a gone pane must refuse → one Failure-semantics bullet added to `specs/herdr-host.md`, `close_all` and `is_reviewr_pane` reworked in `herdr/pane.sh`.
- 2026-07-31: /code-review round → the startup herdr calls must never hold the first paint, the event loop, or the shell prompt, and a genuinely failed close must refuse → bounded/threaded herdr calls in `src/herdr.rs`, close-failure and config-dir bullets in `specs/herdr-host.md` and `specs/config.md`, parallel pane discovery in `herdr/pane.sh`.
- 2026-07-31: /garfield round → five lanes converged on the CLI config-dir lookup still sitting before the first paint, so the fallback now runs on the painted pane and rebuilds from the directory herdr names (`src/lib.rs`); the CLI half gained end-to-end tests, the empty-CLI-answer filter moved into `config_dir_from`, probe markers key by list position, and the missing CHANGELOG bullet landed. Live-verified: `pane list` entries carry no process fields, and `pane_not_found` is the real error code on both reads (`docs/herdr-api-notes.md`).
- 2026-07-31: build-time verification → `pane process-info`, `pane rename --clear`, and `plugin config-dir` all exist on herdr 0.7.5, so `min_herdr_version` stays; the live envelope is `.result.process_info.foreground_processes` and `name` is a rewritable process title, so identity keys on the `argv0`/`argv[0]` basename → `herdr/pane.sh`, `docs/herdr-api-notes.md`.
- 2026-07-31: /review-code round → the label is now the pane's default name, never an override: stamp only an unlabeled pane, clear only a `reviewr` label (user decision; `specs/herdr-host.md` Pane identity, `src/herdr.rs`); the process-info jq refuses a missing envelope key, stderr stays out of the parsed JSON, the installer never replaces a non-symlink, the flag entrypoint logs, and the slow config-dir lookup paints a pending note before its swap.
- 2026-07-31: identity walk plus industry comparison → pane identity switched from label to foreground process (`herdr pane process-info`, verified live) → `specs/herdr-host.md` Pane identity rewritten, DoD and steps 2 and 5 reworded.
- 2026-07-31: release-time verification → the v0.27.0 install's stable links dangled: the build step runs in a staging checkout herdr renames afterwards, so no build-time path survives → link maintenance moved to every action and event (`herdr/pane.sh`), the installer aims at `$HERDR_PLUGIN_ROOT` when set, `specs/herdr-host.md` Install paths rewritten, recut as v0.27.1.
- 2026-07-31: initial plan.
