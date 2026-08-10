# Milestone 1: GitLab — Plan

Delivers `specs/forge-providers.md#gitlab` and the neutral contract in `specs/forge-host.md` (issue #29).
Why now: the forge boundary is the riskiest contract, and GitLab exercises all of it with a known-shape CLI.

## Goal

A GitLab remote — gitlab.com or one `gitlab_host` — resolves its merge request and renders it on the PR tab through `glab`. GitHub behavior is byte-identical to today.

## Definition of Done

- [x] A gitlab.com repository with a published MR shows it: state, draft, fork marker, merge fold, pipeline jobs as checks, discussions and approvals as comments.
- [x] The tab renders GitLab's noun and reference form (`MR !42`) in the chip, footer, and empty states.
- [x] A nested-group path (`group/subgroup/project`) resolves.
- [x] A self-hosted host works after `gitlab_host` is set and `glab auth login --hostname` has run (manual QA).
- [x] Missing `glab` shows its install step. An unauthenticated host shows the `glab auth login --hostname <host>` remedy.
- [x] `gitlab_host` colliding with `github_host` or a built-in host is an invalid config naming both.
- [x] The unsupported-host message points to the per-forge host keys.
- [x] A fetch result never paints after the origin moves to a different forge or hostname (forge-qualified target).
- [x] The chip shows `draft` only while the PR is open.
- [x] Every existing GitHub and GHE test passes unchanged.

Manual QA ran on 2026-07-22 against `gitlab.com/dima169/reviewr-qa!1` in a herdr pane, then against public repositories for the surfaces a fresh account cannot create. `gitlab-org/cli!3531` resolved a fork merge request and read its pipeline from the source project. `gitlab-org/cli!3519` resolved as merged with an approval row, diff findings, and an access-token bot. `gitlab-org/security-products/analyzers/semgrep!732` resolved a draft through a four-segment group path. `salsa.debian.org` resolved through `gitlab_host` and showed the login remedy, because that instance serves its discussions endpoint only to authenticated readers. Removing `glab` from the path showed the install step.

## Out of Scope

- Azure DevOps: provider, `azure_devops_host`, built-in ADO host recognition. Milestone 2.

## Execution Plan

1. [x] `src/config.rs`: add `gitlab_host` (bare hostname, not `gitlab.com`), plus the cross-key and built-in-host collision check with a both-names error. Unit tests beside the existing `github_host` ones.
2. [x] `src/git.rs`: add a `Forge` discriminant to `RepoTarget` and generalize its path to segments (owner/name today, full namespace for GitLab). `classify_remote` matches per forge host set. Extend the classify tests with gitlab.com, `gitlab_host`, nested groups, and collision cases.
3. [x] `src/forge.rs`: split the `gh` call path behind a provider seam keyed by `RepoTarget`'s forge. Implement the GitLab provider over `glab api`: MR resolution via `commits/:sha/merge_requests`, MR detail, head-pipeline jobs, discussions, approvals. Map per `specs/forge-providers.md#gitlab`.
4. [x] `src/forge.rs`: classify `glab` stderr into the missing/unauthenticated/other failure states with per-forge remedy strings.
5. [x] Forge-swap no-paint test: the paint gate already compares `PrFetchInput` by equality, so the forge discriminant added in step 2 qualifies it with no gate change. The test proves it.
6. [x] `src/ui.rs` + `src/forge.rs`: noun and reference form from the resolved forge; `draft` chip only while open.
7. [x] Snapshot-mapping tests with canned `glab` JSON, mirroring the `serde_json` builders beside the `gh` tests in `src/forge.rs` and `tests/pr_candidates.rs`.

## Likely Files

| file            | change                                                        |
| --------------- | ------------------------------------------------------------- |
| `src/config.rs` | `gitlab_host` key, collision validation                       |
| `src/git.rs`    | forge-qualified `RepoTarget`, per-forge `classify_remote`     |
| `src/forge.rs`  | provider seam, `glab` provider, per-forge remedies            |
| `src/lib.rs`    | forge-swap no-paint regression test wiring, if any            |
| `src/ui.rs`     | forge noun and reference form, draft-chip rule                |
| `tests/`        | GitLab fixtures, collision and no-paint cases                 |

## Verification

- `just ci` → green, including every untouched GitHub test.
- `python3 scripts/bench_tui.py --binary target/release/herdr-reviewr --fixture` A/B against `main` → medians unchanged (classify sits on the reload path).
- Manual QA on a real GitLab repository via `just qa-install`, then the user reopens panes.
- Tight: everything the diff adds is exercised by a DoD line. Delete or defer the rest.
- `CFG-WHOLE-FILE` → the host-key collision test in `src/config.rs` → the whole file rejects, naming both keys.
- Gate: promote `pr-tab.md` and `overview.md` to Current. `forge-host.md`, `forge-providers.md`, and `config.md` carry ADO contract and stay Draft until milestone 2; milestone 1's gate verifies their GitHub and GitLab rows against the code.

## Replan

- If `glab api` output diverges from the documented REST shapes, then pin the mapping to observed output and note it in the PR.
- 2026-07-22: QA found GitLab's triage automation posts under plain `-bot` names, which the access-token patterns miss, so its repeated nudges never collapsed → `-bot` joins the bot suffixes → `specs/forge-providers.md` GitLab Comments and `src/gitlab.rs` `is_gitlab_bot`.
- 2026-07-22: review flagged that a target-scoped commit lookup cannot see fork-only commits (upstream bug #289807), leaving an open fork MR unresolvable → landed the name epilogue, where a publication point's branch name nominates merge requests in any state and exact head identity admits them → `src/gitlab.rs` `associate_points` and `specs/forge-providers.md` GitLab Admission.
- 2026-07-22: review found GitLab's commit-to-MR lookup is target-scoped and 404s on unknown commits → the GitLab provider queries the target project and skips unknown commits → landed in `specs/forge-providers.md` (per-forge Admission bullets) and `src/gitlab.rs`.
- 2026-07-22: initial plan.
