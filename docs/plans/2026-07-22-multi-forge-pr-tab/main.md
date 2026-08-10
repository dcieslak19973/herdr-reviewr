# Multi-forge PR tab — Delivery Strategy

Delivers `specs/forge-host.md` and `specs/forge-providers.md`.

## Problem

The PR tab is GitHub-only. A GitLab or Azure DevOps remote classifies as `Unsupported`, so those teams get no PR/MR mirror (issue #29, discussion #30).

## Goal

The remote's host picks the forge. GitLab and Azure DevOps repositories get the same read-only PR tab GitHub has, each through its official CLI.

## Milestone Map

Both milestones ship in one merge and one release. Milestone 1 stops for review, not for a release.

1. GitLab — the forge boundary plus the `glab` provider, delivering issue #29. Ends on a review boundary: the forge-recognition contract is exercised by a second forge before a third arrives.
2. Azure DevOps — the `az` provider, delivering discussion #30. Starts behind an information boundary: the admission behavior of ADO's commit query is unknown until exercised against a real organization.

## Current Milestone

Both milestones are built and QA'd. The branch sits at the merge gate.

## Deferred Decisions

- `azure_devops_host` shipped with milestone 2. The touched specs stay Draft until the merge gate promotes them.

## Milestone 2 constraints

Review of milestone 1 surfaced four places the third forge lands badly, none of which the compiler catches.

- `git::forge_for_host` matches hostnames by equality, so it cannot express `*.visualstudio.com`. Host recognition needs a suffix rule before Azure DevOps arrives.
- `RepoTarget::with_path` splits a URL path into identity segments, but Azure DevOps embeds `_git` and `v3` markers inside its paths. The path needs stripping before validation.
- `config.rs` validates host-key collisions pairwise, which grows quadratically. A third key needs a rule over the whole set.
- `ui.rs` names the forge roster in two literal strings, and `tests/render.rs` asserts only the two current keys, so a forgotten third key passes CI.
- `az` is not installed on the development machine. Milestone 2 installs it before the provider runs.

## Spike: the Azure DevOps commit query

Run 2026-07-22 against `dev.azure.com/dnceng-public/public`, whose project-scoped endpoints read without authentication.

`pullrequestquery` matched a completed pull request from its `lastMergeCommit`, the commit Azure DevOps writes onto the target branch at completion. The same query returned nothing for that pull request's `lastMergeSourceCommit`, and the `commit` query type returned nothing for either commit. The query therefore proves no containment and knows nothing of a pull request before it completes.

Milestone 2 reads this as: a completed pull request resolves through `lastMergeCommit` against absorbed base history, and an active pull request resolves only by listing pull requests and matching a reported `lastMergeSourceCommit` to the pinned `HEAD` or a publication point. That is the enumeration path `specs/forge-providers.md` already contracts, so the spec stands unchanged.

## Replan

- 2026-07-22: initial strategy.
- 2026-07-22: the user chose one merge and one release for both milestones → milestone 1 now ends on a review boundary → the milestone map above.
- 2026-07-22: the milestone 2 spike found `pullrequestquery` resolves completed pull requests only → active pull requests take the specced enumeration path → the spike section above, no spec change.
- 2026-07-22: review flagged open fork MRs as unresolvable if GitLab's commit lookup cannot see fork commits → the GitLab name epilogue nominates merge requests in any state and exact head identity admits them.
- 2026-07-22: the user locked upstream-first precedence across forges → no change, the resolution table stands.
- 2026-07-22: descent-based admission for open ADO pull requests would test ancestry against tips that are rarely local, so it would almost never prove → exact source-tip identity ships alone → `specs/forge-providers.md` Azure DevOps Admission.
- 2026-07-22: `az` refuses every read without credentials, even of public projects → live QA runs after the user's `az login`, with payload shapes grounded through anonymous REST meanwhile.
- 2026-07-23: a second merge-gate review round found ambient triggers dropping mid-flight where `forge-host.md` contracts a superseding fetch → an ambient trigger now rides the in-flight fetch and arms one trailing fetch behind its paint, and the pending request carries its kind (`RefreshKind`) → `src/lib.rs`, `src/app.rs`, `specs/pr-tab.md`, `specs/forge-host.md`.
- 2026-07-23: the same round consolidated the provider kernel — the panic-degrading join, the CLI error mapping, the newest-100 cap, and the optional-surface fold each moved to one home — and fixed the empty-root thread drop in both providers → `src/forge.rs`, `src/gitlab.rs`, `src/azure_devops.rs`. GitLab's unknowable-page-total degradation is now a spec line, not a silent divergence → `specs/forge-host.md`.
- 2026-07-23: the delta re-review found a failed probe orphaning the armed trailing fetch, and the empty-root fix leaving `reply_count` counting empty notes → `probe_failed` folds the trailing mark into its retry, and one `is_comment` predicate backs both the root pick and the count per provider, each with a regression test → `src/lib.rs`, `src/gitlab.rs`, `src/azure_devops.rs`.
- 2026-07-23: the garfield end-to-end gate found no blocker or high concern → the capped-jobs `pipeline` row and the enumeration asymmetry became spec lines, an unreachable evaluations refetch dropped, the undated-bot-review dedup obligation gained its test, and stale GitHub-only wording plus over-wide visibility cleaned up → `specs/forge-providers.md`, `src/forge.rs`, `src/lib.rs`, `src/azure_devops.rs`, `AGENTS.md`.
- 2026-07-23: a fresh single-pass review found the ADO merged epilogue unreachable while parked on the branch tip — the commit query matches only the merge commit, which the branch's own history never contains → a completed enumeration now admits by exact source tip, the POST body file is created fresh and owner-only, and a self-hosted virtual directory joins the collection → `src/azure_devops.rs`, `src/git.rs`, `specs/forge-providers.md`.
- 2026-07-23: a cleanup pass found the ADO detail read redundant for enumeration-admitted picks — the list nodes carry every consumed field, verified live — so the picked node now rides the wave and only a query-admitted pick pays `az repos pr show` → `src/azure_devops.rs`, plus small consistency fixes → `src/gitlab.rs`, `src/ui.rs`, `src/lib.rs`.
- 2026-07-23: a rerun opus review plus a blind-ladder reset judged the branch's shape right and its residue coherence debt → the pick ladder and sync wrapper each live once in the kernel (`resolve_pick`, `local_sync`), the admitted node travels on `AssocPr.raw` instead of a side-map, the built-in ADO host set has one authority, and four facts moved to their spec homes (the commanded-refresh carve-out, the GitLab page-ceiling datum, two trimmed rationale clauses, the virtual-directory full-form rule) → `src/forge.rs`, `src/gitlab.rs`, `src/azure_devops.rs`, `src/git.rs`, `specs/forge-host.md`, `specs/forge-providers.md`.
- 2026-07-23: the user ruled the 100-row enumeration bound a practical assumption, and an end-to-end audit applied that stance branch-wide → the ADO commit query went (with the POST temp-file machinery, the detail read, and the project-GUID URL fallback it alone consumed), the abandoned enumeration went (Azure DevOps has no closed epilogue), and the on-prem virtual-directory support went (a vdir remote reads as `Malformed`) → `src/azure_devops.rs`, `src/git.rs`, `src/forge.rs`, `specs/forge-providers.md`. Every ADO fetch is now two association reads plus three surface reads, all enumeration-admitted.
