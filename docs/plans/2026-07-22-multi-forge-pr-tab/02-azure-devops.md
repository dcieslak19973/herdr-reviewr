# Milestone 2: Azure DevOps — Plan

Builds on `01-gitlab`. The forge boundary (`Forge`, `RepoTarget`, the provider dispatch, `ForgeHosts`) is on the branch and reviewed. Nothing is frozen: both milestones ship in one merge and one release.
Why now: the spike resolved the admission unknown, so the provider's shape is fully specified.

## Goal

An Azure DevOps remote — `dev.azure.com`, `{org}.visualstudio.com`, or one `azure_devops_host` — resolves its pull request and renders it on the PR tab through `az`. GitHub and GitLab behavior is byte-identical to milestone 1.

## Definition of Done

- [x] A `dev.azure.com` repository with a published PR shows it: state, draft, fork marker, merge fold, policy evaluations and commit statuses as checks, threads and votes as comments.
- [x] Every accepted URL form resolves to one target: the `_git` https form, the `v3` ssh forms, and the legacy `{org}.visualstudio.com` equivalents.
- [x] A completed PR resolves through an exact source-tip match over the completed enumeration, an absorbed tip included.
- [x] An active PR resolves through an exact source-tip match against the pinned `HEAD` or a publication point.
- [x] Missing `az` shows its install step. A missing `azure-devops` extension shows `az extension add --name azure-devops`. An unauthenticated organization shows the `az login` remedy.
- [x] `azure_devops_host` colliding with another host key or a built-in host, `*.visualstudio.com` included, is an invalid config naming both.
- [x] The needs-remote and unsupported-host messages name all three forges and all three host keys.
- [x] A file-position thread renders as a finding with its `path:line` anchor and resolved marker. A build-service identity counts as a bot.
- [x] Every existing GitHub and GitLab test passes unchanged.

Manual QA ran on 2026-07-22 against `dev.azure.com/extruct/Extruct AI/_git/reviewr-qa`, a fixture repository built for it. The active `#1` showed its policy evaluation and commit status as checks, a resolved `lib.rs:2` finding, a comment, a vote row, and a real conflict fold. Parking on `main` resolved the completed `#2` through its merge commit, `feature-abandoned` resolved the abandoned `#3`, and `feature-draft` showed the `draft` chip on `#4`. A cross-repository pull request from a server-side fork resolved as `⑂ fork-feature #5`. The spaced project name exercised percent-decoding end to end. Removing `az` from the path showed the install step, and an unknown organization showed the retryable error.

The 2026-07-23 purge removed merge-commit resolution and abandoned-PR resolution. Re-verified against the live fixture the same day, against the real `az repos pr list` payloads and the post-purge admission logic. The active `#1` enumeration node carries every field the snapshot reads (`description`, `mergeStatus`, `reviewers` with votes, `repository.project.id`, `forkSource`), so the dropped `az repos pr show` detail read loses nothing. Parking on `feature-completed` resolves the completed `#2` by its source tip `ad290950`. Parking on `main` no longer resolves `#2`, because admission is by source tip and `main` absorbs only the merge commit `566f26be`. `feature-abandoned` no longer resolves the abandoned `#3`. The TUI render was not eyeballed.

## Out of Scope

- Descent-based admission for active PRs ("provably descends from"). Exact identity ships first. The deferral is logged in `main.md`.

## Execution Plan

1. [x] Probe: install `az` and the `azure-devops` extension, capture the missing-extension and auth-failure stderr, and verify each read against `dev.azure.com/dnceng-public/public` — PR list, PR detail, `pullrequestquery` POST, threads, policy evaluations, commit statuses. Record whether `az` reads public data anonymously.
2. [x] `src/git.rs`: the `AzureDevOps` variant with its vocabulary, `ForgeHosts.azure_devops`, the `*.visualstudio.com` suffix rule in `forge_for_host`, and path shaping in `with_path`/`classify_remote` — strip `_git`, `v3`, and `DefaultCollection`, normalize `ssh.dev.azure.com` to `dev.azure.com` and hoist the `visualstudio.com` org. Unit tests per URL form, positive and negative.
3. [x] `src/config.rs`: the `azure_devops_host` key through `parse_forge_host`, and the collision check generalized from the pairwise pair to a whole-set scan. Tests cover the three collisions and the wildcard rejection.
4. [x] `src/ui.rs` + `tests/render.rs`: the two roster strings gain the third forge and key, the render test asserts it.
5. [x] `src/forge.rs`: the `AzureDevOps` dispatch arm, the `NoExtension` view with its remedy, and `Forge::login_hint` so the ADO remedy is `az login` while GitHub and GitLab keep theirs.
6. [x] `src/azure_devops.rs`: the provider mirroring `src/gitlab.rs` — runners pinned to `--organization`, the split admission (merged via `pullrequestquery`, active via enumeration and exact source-tip match), state/merge/sync/checks/comments mappings per `specs/forge-providers.md#azure-devops`, the stderr classifier grounded in step 1's captures, `is_azure_bot`.
7. [x] Inline `json!()` unit tests per pure mapping, plus render tests for the ADO failure states and vocabulary.

## Likely Files

| file                  | change                                                       |
| --------------------- | ------------------------------------------------------------ |
| `src/git.rs`          | `AzureDevOps` variant, suffix rule, ADO path shaping         |
| `src/config.rs`       | `azure_devops_host` key, whole-set collision scan            |
| `src/forge.rs`        | dispatch arm, `NoExtension` view, `login_hint`               |
| `src/azure_devops.rs` | the `az` provider (new)                                      |
| `src/ui.rs`           | the two forge-roster strings                                 |
| `tests/render.rs`     | third key assertion, ADO failure-state and vocabulary renders |

## Verification

- `just ci` → green, including every untouched GitHub and GitLab test.
- `python3 scripts/bench_tui.py --fixture` A/B against the pre-branch `main` on 2026-07-23, interleaved → painted medians unchanged within run-to-run drift (the classify additions sit on the fetch-probe path, not the frame path).
- Manual QA against `dnceng-public/public`, or against a user-created organization when `az` demands login: completed-PR resolution from absorbed history, active-PR resolution by source tip, a real thread and build-service identity, then `just qa-install` and the user reopens panes.
- Tight: everything the diff adds is exercised by a DoD line. Delete or defer the rest.
- Gate: promote `forge-host.md`, `forge-providers.md`, and `config.md` to Current with the rest at the merge gate.

## Replan

- If `az` refuses anonymous reads of public projects, then QA moves to a user-created organization after `az login`.
- If an `az devops invoke` route or api-version differs from the assumed shape, then pin the call to observed behavior before writing the provider.
- 2026-07-22: initial plan.
- 2026-07-22: the QA project name carries a space, which the remote URL percent-encodes and the segment validation rejected → Azure DevOps segments percent-decode and admit spaces → `src/git.rs` `ado_canonicalize` and `valid_ado_component`.
- 2026-07-22: `az devops invoke` rejects the dotted preview api-version (`7.1-preview.1`) → the evaluations read pins `7.1-preview` → `src/azure_devops.rs`.
- 2026-07-22: a fork pull request reports the virtual `refs/pull/{id}/source` as its source ref and keeps the branch on `forkSource.name` → `head_ref_of` prefers the fork branch → `src/azure_devops.rs`.
- 2026-07-22: an organization the credential cannot read answers "requires user authentication", which the classifier missed → the wording joins the unauthenticated patterns → `src/azure_devops.rs` `classify_failure`.
- 2026-07-23: the tab painted in ~5s on Azure DevOps — each `az` call costs ~1s of CLI startup, the provider ran three serial waves, and entering the tab discarded the in-flight startup fetch and repeated it → the provider collapsed to two concurrent waves (the abandoned probe and the detail joined existing waves, the project GUID rides the association nodes), and ambient refresh triggers now ride an in-flight fetch while the `refresh` key still forces a fresh one → `src/azure_devops.rs`, `src/lib.rs` `request_refresh`. Paint fell to ~1.5–2s cold and near-instant when the startup fetch has landed.
