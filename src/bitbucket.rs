//! Read-only Bitbucket Data Center access via `curl`: the pull request's identity, state,
//! checks, and comments.
//!
//! The Bitbucket provider behind `src/forge.rs` (`specs/forge-providers.md`). It follows the
//! neutral resolution contract in `specs/forge-host.md` — the branch's forge names list pull
//! requests by `at=refs/heads/<name>` — through explicitly hosted REST calls under
//! `/rest/api/latest`, and fills the same normalized [`PrSnapshot`] the other providers do. It
//! never writes to Bitbucket. Unlike `gh`/`glab`/`az`, there is no Bitbucket CLI, so this
//! backend authenticates itself: a bearer token from `BITBUCKET_TOKEN` or `git credential
//! fill`, carried to `curl` on stdin so it never appears in argv (`ps`/`/proc` visible args).

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde_json::Value;

use crate::forge::{
    AssocPr, Association, Check, CheckStatus, Comment, CommentKind, Merge, PrFetchInput,
    PrSnapshot, PrState, PrView, Sync, finish_comments, push_unique, upsert_latest,
};

/// Read Bitbucket for one already-derived input. Degradation stays in-band for the PR tab.
pub(crate) fn fetch(
    repo: &Path,
    input: &PrFetchInput,
    target: &crate::git::RepoTarget,
    cancelled: &AtomicBool,
) -> PrView {
    match fetch_inner(repo, input, target, cancelled) {
        Ok(view) => view,
        Err(error) => error.into_view(target.host()),
    }
}

/// A classified Bitbucket failure, mapped to a [`PrView`] degraded state.
#[derive(Debug)]
enum BitbucketError {
    /// `curl` is not on `PATH`.
    NoCurl,
    /// No usable `BITBUCKET_TOKEN`/git-credential, or the endpoint answered 401/403.
    NotAuthed,
    LocalGit(String),
    Other(String),
}

impl BitbucketError {
    fn into_view(self, host: &str) -> PrView {
        match self {
            Self::NoCurl => PrView::NoCli(crate::git::Forge::Bitbucket),
            Self::NotAuthed => PrView::NotAuthed(crate::git::Forge::Bitbucket, host.to_owned()),
            Self::LocalGit(message) => PrView::GitError(message),
            Self::Other(message) => PrView::Error(crate::git::Forge::Bitbucket, message),
        }
    }
}

/// The retryable error a panicked reader degrades into (`crate::forge::join_read`).
fn died(surface: &str) -> BitbucketError {
    BitbucketError::Other(format!("{surface} read panicked"))
}

/// The project/repo/host/auth context every REST call in one fetch shares.
struct Ctx<'a> {
    repo: &'a Path,
    host: &'a str,
    owner: &'a str,
    name: &'a str,
    token: &'a str,
    cancelled: &'a AtomicBool,
}

impl Ctx<'_> {
    /// The project/repo base every REST call under `/rest/api/latest` shares: `owner` is the
    /// project key, `name` the repo slug.
    fn base(&self) -> String {
        format!(
            "https://{}/rest/api/latest/projects/{}/repos/{}",
            self.host,
            crate::forge::urlencode(self.owner),
            crate::forge::urlencode(self.name)
        )
    }

    fn get(&self, url: &str) -> Result<Value, BitbucketError> {
        curl_get(self.repo, self.token, url, self.cancelled)
    }
}

fn fetch_inner(
    repo: &Path,
    input: &PrFetchInput,
    target: &crate::git::RepoTarget,
    cancelled: &AtomicBool,
) -> Result<PrView, BitbucketError> {
    let host = target.host();
    let token = resolve_token(repo, host, cancelled)?;
    let head = input.local.head_oid.as_deref();

    let target_ctx =
        Ctx { repo, host, owner: target.owner(), name: target.name(), token: &token, cancelled };
    let (assoc, mut truncated) = associate_by_branch(&target_ctx, &input.local.names)?;
    let mut pick = crate::forge::resolve_pick(repo, &assoc, head)
        .map_err(|error| BitbucketError::LocalGit(error.0))?
        .map(|number| (number, target.owner().to_string(), target.name().to_string()));

    // A fork clone: `origin` is the fork, the target is upstream. Both repositories are
    // asked, and upstream's pick outranks the fork's own (`specs/forge-host.md`).
    if pick.is_none()
        && let Some(fork) = crate::forge::fork_repository(input.origin_repository.as_ref(), target)
    {
        let fork_ctx =
            Ctx { repo, host, owner: fork.owner(), name: fork.name(), token: &token, cancelled };
        let (fork_assoc, fork_truncated) = associate_by_branch(&fork_ctx, &input.local.names)?;
        truncated |= fork_truncated;
        pick = crate::forge::resolve_pick(repo, &fork_assoc, head)
            .map_err(|error| BitbucketError::LocalGit(error.0))?
            .map(|number| (number, fork.owner().to_string(), fork.name().to_string()));
    }

    let Some((id, owner, name)) = pick else {
        return Ok(PrView::NoPr);
    };
    let ctx = Ctx { repo, host, owner: &owner, name: &name, token: &token, cancelled };

    let detail = ctx.get(&format!("{}/pull-requests/{id}", ctx.base()))?;
    if detail["id"].as_u64().is_none() {
        return Ok(PrView::NoPr);
    }

    // Sync compares the fetch's pinned HEAD to the PR head, so a checkout or commit landing
    // mid-fetch never pairs one branch's PR with another branch's count.
    let head_sha = detail["fromRef"]["latestCommit"].as_str().unwrap_or_default();
    let sync = crate::forge::local_sync(repo, input.local.head_oid.as_deref(), head_sha)
        .map_err(|error| BitbucketError::LocalGit(error.0))?;

    // The merge, checks, and comments reads are independent; running them concurrently keeps
    // the fetch's wall clock at the slowest single read, not their sum.
    let (merge, checks_result, comments_result) = thread::scope(|scope| {
        let merge_handle = scope.spawn(|| {
            // Mergeability only makes sense (and is only queried) while the PR is open.
            if detail["state"].as_str() == Some("OPEN") {
                fetch_merge(&ctx, id)
            } else {
                Ok(Merge::Clean)
            }
        });
        let checks_handle = scope.spawn(|| {
            if head_sha.is_empty() { Ok((Vec::new(), false)) } else { fetch_checks(&ctx, head_sha) }
        });
        let comments_handle = scope.spawn(|| fetch_comments(&ctx, id));
        (
            crate::forge::join_read(merge_handle, || died("merge")),
            crate::forge::join_read(checks_handle, || died("checks")),
            crate::forge::join_read(comments_handle, || died("comments")),
        )
    });
    let merge = merge?;
    let (checks, checks_capped) = checks_result?;
    let (comments, comments_capped) = comments_result?;
    truncated |= checks_capped || comments_capped;

    Ok(PrView::Pr(Box::new(build_snapshot(&detail, merge, checks, comments, sync, truncated))))
}

// ---- Association (branch resolution) -------------------------------------------------

/// The branch's pull requests in one project: an OPEN listing and a historical listing
/// (`state=ALL`, with `OPEN` rows dropped client-side — Bitbucket DC's REST API has no
/// combined "finished" state filter), one REST call each per candidate name, run
/// concurrently and folded into one [`Association`] for `crate::forge::resolve_pick`.
/// Returns the association and whether any listing page was truncated.
fn associate_by_branch(
    ctx: &Ctx<'_>,
    names: &[String],
) -> Result<(Association, bool), BitbucketError> {
    let base = ctx.base();
    let open_urls = names.iter().map(|branch| {
        format!(
            "{base}/pull-requests?state=OPEN&direction=OUTGOING&at=refs/heads/{}&limit=100",
            crate::forge::urlencode(branch)
        )
    });
    let hist_urls = names.iter().map(|branch| {
        format!(
            "{base}/pull-requests?state=ALL&direction=OUTGOING&at=refs/heads/{}&limit=20",
            crate::forge::urlencode(branch)
        )
    });
    let urls: Vec<String> = open_urls.chain(hist_urls).collect();
    let mut results = curl_get_fan_out(ctx, &urls).into_iter();

    let mut assoc = Association::default();
    let mut truncated = false;
    for _ in names {
        let v = results.next().expect("one open listing per candidate")?;
        truncated |= is_truncated(&v);
        for p in v["values"].as_array().into_iter().flatten() {
            if let Some(pr) = assoc_pr(p, false) {
                push_unique(&mut assoc.open, pr);
            }
        }
    }
    for _ in names {
        let v = results.next().expect("one historical listing per candidate")?;
        truncated |= is_truncated(&v);
        for p in v["values"].as_array().into_iter().flatten() {
            if p["state"].as_str() == Some("OPEN") {
                continue;
            }
            if let Some(pr) = assoc_pr(p, true) {
                push_unique(&mut assoc.history, pr);
            }
        }
    }
    Ok((assoc, truncated))
}

/// Run several `curl_get` reads concurrently, returning their results in call order.
/// Wall-clock is the slowest single read — each candidate branch's REST calls are
/// independent, so nothing is gained by serializing them.
fn curl_get_fan_out(ctx: &Ctx<'_>, urls: &[String]) -> Vec<Result<Value, BitbucketError>> {
    thread::scope(|scope| {
        let handles: Vec<_> = urls.iter().map(|url| scope.spawn(move || ctx.get(url))).collect();
        handles.into_iter().map(|h| crate::forge::join_read(h, || died("listing"))).collect()
    })
}

/// One listing node reduced to the pick-relevant fields shared with the other providers.
/// `historical` selects the `closedDate` (falling back to `updatedDate`) as the history sort
/// key; an open row carries no close time.
fn assoc_pr(p: &Value, historical: bool) -> Option<AssocPr> {
    let closed_at = if historical {
        p["closedDate"]
            .as_i64()
            .or_else(|| p["updatedDate"].as_i64())
            .map(epoch_ms_to_iso)
            .unwrap_or_default()
    } else {
        String::new()
    };
    Some(AssocPr {
        number: p["id"].as_u64()?,
        head_oid: p["fromRef"]["latestCommit"].as_str().unwrap_or_default().to_string(),
        head_ref: p["fromRef"]["displayId"].as_str().unwrap_or_default().to_string(),
        created_at: p["createdDate"].as_i64().map(epoch_ms_to_iso).unwrap_or_default(),
        closed_at,
        raw: None,
    })
}

// ---- Detail reads ----------------------------------------------------------------------

/// The PR's mergeability, folded to a [`Merge`]. Only queried while the PR is open.
fn fetch_merge(ctx: &Ctx<'_>, id: u64) -> Result<Merge, BitbucketError> {
    let v = ctx.get(&format!("{}/pull-requests/{id}/merge", ctx.base()))?;
    Ok(derive_merge(&v))
}

/// The head commit's build statuses, normalised to [`Check`]s. This endpoint lives outside
/// the project/repo path — it is keyed by commit hash alone, shared across every repo on the
/// host. `/latest/` already returns one status per build key, but `upsert_latest` keeps the
/// assembly consistent with every other provider's checks list.
fn fetch_checks(ctx: &Ctx<'_>, sha: &str) -> Result<(Vec<Check>, bool), BitbucketError> {
    let url = format!("https://{}/rest/build-status/latest/commits/{sha}?limit=100", ctx.host);
    let v = ctx.get(&url)?;
    let mut checks: Vec<Check> = Vec::new();
    for c in v["values"].as_array().into_iter().flatten() {
        let Some(name) = c["name"].as_str().or_else(|| c["key"].as_str()) else { continue };
        let status = check_status(c["state"].as_str().unwrap_or(""));
        upsert_latest(&mut checks, Check { name: name.to_string(), status });
    }
    Ok((checks, is_truncated(&v)))
}

/// The PR's activity feed, filtered to comments and normalised to [`Comment`]s.
fn fetch_comments(ctx: &Ctx<'_>, id: u64) -> Result<(Vec<Comment>, bool), BitbucketError> {
    let url = format!("{}/pull-requests/{id}/activities?limit=100", ctx.base());
    let v = ctx.get(&url)?;
    Ok((map_activities(&v["values"]), is_truncated(&v)))
}

// ---- Auth ---------------------------------------------------------------------------------

/// Resolve the bearer token for `host`: `BITBUCKET_TOKEN` first, else `git credential fill`,
/// else [`BitbucketError::NotAuthed`]. The env var is read fresh on every fetch — never
/// cached globally — so a rotated token takes effect on the next poll without a restart. The
/// credential helper is only spawned when the env var is absent/empty, so a poll with
/// `BITBUCKET_TOKEN` set never pays for a `git credential fill` subprocess.
fn resolve_token(repo: &Path, host: &str, cancelled: &AtomicBool) -> Result<String, BitbucketError> {
    let env = std::env::var("BITBUCKET_TOKEN").ok();
    token_from(env, || credential_password(repo, host, cancelled)).map_err(|()| BitbucketError::NotAuthed)
}

/// The pure auth decision, independent of how `env`/`credential_password` were obtained: a
/// non-empty env var wins, then a non-empty credential-helper password, else no token.
/// `credential_password` is only invoked when `env` is absent/empty, so callers can pass a
/// closure that spawns a subprocess without paying for it on the common env-var-set path.
fn token_from(env: Option<String>, credential_password: impl FnOnce() -> Option<String>) -> Result<String, ()> {
    if let Some(t) = env.filter(|s| !s.is_empty()) {
        return Ok(t);
    }
    if let Some(p) = credential_password().filter(|s| !s.is_empty()) {
        return Ok(p);
    }
    Err(())
}

/// Ask `git credential fill` for the password it has stored for `host` over HTTPS, or `None`
/// on any failure (tool missing, cancelled, no matching credential). Thin and untested — the
/// decision logic lives in [`token_from`].
///
/// Runs with `GIT_TERMINAL_PROMPT=0` and `GIT_ASKPASS` cleared (set to empty): without a
/// helper that already has the credential, `git credential fill` otherwise falls back to
/// prompting `Password for ...` directly on `/dev/tty`, bypassing stdio entirely. This pane
/// owns the terminal — a prompt racing the ratatui redraw loop would corrupt the UI and block
/// this fetch worker on input nobody can supply. Suppressing the prompt makes the call fail
/// cleanly instead, so a missing credential degrades to [`BitbucketError::NotAuthed`] rather
/// than hanging.
fn credential_password(repo: &Path, host: &str, cancelled: &AtomicBool) -> Option<String> {
    let input = format!("protocol=https\nhost={host}\n\n");
    let out = run_tool(
        "git",
        repo,
        &["credential", "fill"],
        Some(&input),
        &[("GIT_TERMINAL_PROMPT", "0"), ("GIT_ASKPASS", "")],
        cancelled,
    )
    .ok()?;
    out.lines().find_map(|l| l.strip_prefix("password=").map(str::to_string))
}

// ---- Transport -----------------------------------------------------------------------------

/// GET `url` with the bearer token via a stdin curl config, so the token is invisible to
/// `ps`/`/proc`. `--fail` maps HTTP errors to exit 22 with the status line on stderr.
fn curl_get(repo: &Path, token: &str, url: &str, cancelled: &AtomicBool) -> Result<Value, BitbucketError> {
    let config = format!("header = \"Authorization: Bearer {token}\"\n");
    let out = run_tool(
        "curl",
        repo,
        &["--silent", "--show-error", "--fail", "--config", "-", url],
        Some(&config),
        &[],
        cancelled,
    )
    .map_err(classify)?;
    serde_json::from_str(&out).map_err(|e| BitbucketError::Other(e.to_string()))
}

/// `NotFound` → `NoCli(curl)`; a cancelled/IO failure → `Other`; a failed run's stderr
/// containing "401"/"403" → `NotAuthed`; anything else → `Other`. `curl` has no stable exit
/// code for "unauthenticated" beyond the generic `--fail` 22, so this reads stderr.
fn classify(f: SpawnFail) -> BitbucketError {
    match f {
        SpawnFail::NotFound => BitbucketError::NoCurl,
        SpawnFail::Cancelled => BitbucketError::Other("request cancelled".to_string()),
        SpawnFail::Io(message) => BitbucketError::Other(message),
        SpawnFail::Failed { stderr } => {
            let s = stderr.to_lowercase();
            if s.contains("401") || s.contains("403") {
                BitbucketError::NotAuthed
            } else {
                BitbucketError::Other(stderr.trim().to_string())
            }
        }
    }
}

/// How a spawned `curl`/`git` subprocess failed, before Bitbucket-specific classification.
/// Bitbucket has no CLI of its own — `curl` and `git credential fill` are run the same way
/// `gh`/`glab`/`az` are run by `crate::forge::run_provider`, but both need a secret written to
/// stdin instead of argv, which that shared runner does not support — so this backend runs
/// its own subprocesses directly.
enum SpawnFail {
    NotFound,
    Failed { stderr: String },
    Cancelled,
    Io(String),
}

/// Spawn `tool` with `args` in `repo`, optionally writing `stdin` to the child and applying
/// `envs` on top of the inherited environment, draining both pipes while polling `cancelled`
/// so a large response cannot fill a pipe and block the child before it exits. A superseded
/// config/fetch kills the process; the coordinator keeps ownership until this worker reports
/// completion, preserving one real fetch in flight.
fn run_tool(
    tool: &str,
    repo: &Path,
    args: &[&str],
    stdin: Option<&str>,
    envs: &[(&str, &str)],
    cancelled: &AtomicBool,
) -> Result<String, SpawnFail> {
    let mut cmd = Command::new(tool);
    cmd.current_dir(repo).args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (key, value) in envs {
        cmd.env(key, value);
    }
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(SpawnFail::NotFound),
        Err(e) => return Err(SpawnFail::Io(e.to_string())),
    };
    if let Some(input) = stdin {
        let mut pipe = child.stdin.take().expect("piped stdin");
        let _ = pipe.write_all(input.as_bytes());
        drop(pipe); // close so the child sees EOF
    }

    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });
    let status = loop {
        if cancelled.load(Ordering::Acquire) {
            let _ = child.kill();
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(SpawnFail::Io(error.to_string()));
            }
        }
    };
    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    if cancelled.load(Ordering::Acquire) {
        return Err(SpawnFail::Cancelled);
    }
    if status.success() {
        return Ok(String::from_utf8_lossy(&stdout).into_owned());
    }
    Err(SpawnFail::Failed { stderr: String::from_utf8_lossy(&stderr).into_owned() })
}

/// Whether a Bitbucket listing response's page was not the last — REST DC pages every listing
/// with `isLastPage`; `false` means a fuller surface exists past the requested `limit`.
fn is_truncated(v: &Value) -> bool {
    v["isLastPage"].as_bool() == Some(false)
}

// ---- Pure normalization (unit-tested) --------------------------------------------------

/// OPEN → Open, MERGED → Merged, DECLINED → Closed (default Open, like the other backends'
/// `parse_state`, so an unrecognised lifecycle still reads as live rather than vanishing).
fn parse_state(s: &str) -> PrState {
    match s {
        "MERGED" => PrState::Merged,
        "DECLINED" => PrState::Closed,
        _ => PrState::Open,
    }
}

/// Fold the `/merge` endpoint's `{canMerge, conflicted, vetoes[]}` into a [`Merge`]. A real
/// conflict always wins; a veto without `canMerge` is a blocking gate a reviewer can act on;
/// everything else (including a still-computing response) reads as `Clean`.
fn derive_merge(v: &Value) -> Merge {
    let conflicted = v["conflicted"].as_bool().unwrap_or(false);
    let can_merge = v["canMerge"].as_bool().unwrap_or(true);
    let vetoes_empty = v["vetoes"].as_array().is_none_or(Vec::is_empty);
    if conflicted {
        Merge::Conflicting
    } else if !can_merge && !vetoes_empty {
        Merge::Blocked
    } else {
        Merge::Clean
    }
}

/// Normalise a build-status `state` to a [`CheckStatus`]. `UNKNOWN` and any other value read
/// as still-pending, matching the siblings' "unrecognised = pending" default.
fn check_status(s: &str) -> CheckStatus {
    match s {
        "SUCCESSFUL" => CheckStatus::Success,
        "FAILED" => CheckStatus::Failure,
        "INPROGRESS" => CheckStatus::Running,
        "CANCELLED" => CheckStatus::Skipped,
        _ => CheckStatus::Pending,
    }
}

/// `createdDate`/`closedDate` are epoch milliseconds → ISO-8601 `YYYY-MM-DDTHH:MM:SSZ`, so
/// sorting and `relative_age` work unchanged. Inverse of `crate::forge::parse_iso`'s
/// civil-date algorithm, via Howard Hinnant's `civil_from_days`.
// The civil-from-days algorithm reads naturally with the conventional short field names.
#[allow(clippy::many_single_char_names, clippy::similar_names)]
fn epoch_ms_to_iso(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86400);
    let tod = secs.rem_euclid(86400);
    let (h, mi, se) = (tod / 3600, (tod % 3600) / 60, tod % 60);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = mp + if mp < 10 { 3 } else { -9 }; // [1, 12]
    let y = y + i64::from(m <= 2);

    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{se:02}Z")
}

/// Whether a comment's author is a service account: `author.type == "SERVICE"` when the
/// field is present — the authoritative signal, and the only one consulted once it exists, so
/// a normal user named e.g. `release-bot` never misreads as a bot just because a `type`
/// happens to be reported. Without a `type` field (older DC versions), the shared name-only
/// heuristics (`[bot]`/`-bot` suffix) apply, plus a `_bot` suffix — Bitbucket's own
/// convention for scripted accounts.
fn author_is_bot(author: &Value) -> bool {
    match author["type"].as_str() {
        Some(t) => t == "SERVICE",
        None => {
            let name = author["name"].as_str().unwrap_or("");
            crate::forge::is_named_bot(name) || name.ends_with("_bot")
        }
    }
}

/// An activity with a `commentAnchor{path, line}` → [`CommentKind::Finding`] (anchor
/// `path:line`; a missing `line` anchors to the path alone; `anchor.orphaned == true` marks
/// it outdated). `comment.state == "RESOLVED"` or `comment.threadResolved == true` → resolved.
/// `reply_count` is the total nested `comment.comments[]` entries, counted recursively (a
/// reply can itself carry replies). Without an anchor → [`CommentKind::Comment`]. Bitbucket
/// has no review-body surface, so there is no `Review` kind here. `author` is
/// `comment.author.name`. Non-`COMMENTED` activities (opened, approved, rescoped, …) are
/// dropped. `finish_comments` collapses a bot's repeated plain comments to its latest and
/// orders the result newest-first, matching the other providers.
fn map_activities(activities: &Value) -> Vec<Comment> {
    let mut out = Vec::new();
    for a in activities.as_array().into_iter().flatten() {
        if a["action"].as_str() != Some("COMMENTED") {
            continue;
        }
        let comment = &a["comment"];
        if comment.is_null() {
            continue;
        }
        let author = &comment["author"];
        let author_name = author["name"].as_str().unwrap_or("").to_string();
        let is_bot = author_is_bot(author);
        let body = comment["text"].as_str().unwrap_or("").trim().to_string();
        let created_at = comment["createdDate"].as_i64().map(epoch_ms_to_iso).unwrap_or_default();
        let is_resolved = comment["state"].as_str() == Some("RESOLVED")
            || comment["threadResolved"].as_bool().unwrap_or(false);
        let reply_count = count_replies(comment);

        let anchor = a.get("commentAnchor").filter(|v| !v.is_null());
        let c = if let Some(anchor) = anchor {
            let path = anchor["path"].as_str().unwrap_or("");
            let anchor_str = match anchor["line"].as_u64() {
                Some(line) => format!("{path}:{line}"),
                None => path.to_string(),
            };
            Comment {
                kind: CommentKind::Finding,
                author: author_name,
                author_is_bot: is_bot,
                anchor: anchor_str,
                body,
                snippet: None,
                created_at,
                is_resolved,
                is_outdated: anchor["orphaned"].as_bool().unwrap_or(false),
                reply_count,
            }
        } else {
            Comment {
                kind: CommentKind::Comment,
                author: author_name,
                author_is_bot: is_bot,
                anchor: "comment".to_string(),
                body,
                snippet: None,
                created_at,
                is_resolved,
                is_outdated: false,
                reply_count,
            }
        };
        out.push(c);
    }
    finish_comments(&mut out);
    out
}

/// Count every nested `comments[]` entry under `comment`, recursively — a reply can itself
/// carry replies, so this is the total thread size below the root, not just its direct replies.
fn count_replies(comment: &Value) -> u32 {
    let Some(arr) = comment["comments"].as_array() else {
        return 0;
    };
    let direct = u32::try_from(arr.len()).unwrap_or(u32::MAX);
    direct + arr.iter().map(count_replies).sum::<u32>()
}

/// Snapshot assembly from detail + merge + checks + comments + sync. `number` is `id`;
/// `head_ref`/`base_ref` are `fromRef`/`toRef`'s `displayId`; `head_oid` is
/// `fromRef.latestCommit`. A fork is detected by comparing the PR's own `fromRef.repository`
/// to its `toRef.repository` (always the queried repo) — case-insensitive on the project
/// key, exact on the slug — so no external target is needed here. `draft` defaults to
/// `false` when absent (older DC versions predate the field).
fn build_snapshot(
    detail: &Value,
    merge: Merge,
    checks: Vec<Check>,
    comments: Vec<Comment>,
    sync: Sync,
    truncated: bool,
) -> PrSnapshot {
    let from_repo = &detail["fromRef"]["repository"];
    let to_repo = &detail["toRef"]["repository"];
    let from_key = from_repo["project"]["key"].as_str().unwrap_or("");
    let to_key = to_repo["project"]["key"].as_str().unwrap_or("");
    let from_slug = from_repo["slug"].as_str().unwrap_or("");
    let to_slug = to_repo["slug"].as_str().unwrap_or("");
    let head_is_fork = !from_key.eq_ignore_ascii_case(to_key) || from_slug != to_slug;

    PrSnapshot {
        number: detail["id"].as_u64().unwrap_or_default(),
        title: detail["title"].as_str().unwrap_or_default().to_string(),
        url: detail["links"]["self"][0]["href"].as_str().unwrap_or_default().to_string(),
        body: detail["description"].as_str().unwrap_or_default().to_string(),
        state: parse_state(detail["state"].as_str().unwrap_or("OPEN")),
        is_draft: detail["draft"].as_bool().unwrap_or(false),
        head_ref: detail["fromRef"]["displayId"].as_str().unwrap_or_default().to_string(),
        head_is_fork,
        head_oid: detail["fromRef"]["latestCommit"].as_str().unwrap_or_default().to_string(),
        base_ref: detail["toRef"]["displayId"].as_str().unwrap_or_default().to_string(),
        merge,
        sync,
        checks,
        comments,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_state_maps_the_three_bitbucket_lifecycles() {
        assert_eq!(parse_state("OPEN"), PrState::Open);
        assert_eq!(parse_state("MERGED"), PrState::Merged);
        assert_eq!(parse_state("DECLINED"), PrState::Closed);
        assert_eq!(parse_state("anything-else"), PrState::Open); // default is the live case
    }

    #[test]
    fn derive_merge_folds_conflicted_and_vetoed_but_not_a_clean_response() {
        assert_eq!(
            derive_merge(&serde_json::json!({"canMerge": false, "conflicted": true, "vetoes": []})),
            Merge::Conflicting
        );
        assert_eq!(
            derive_merge(
                &serde_json::json!({"canMerge": false, "conflicted": false, "vetoes": [{"summaryMessage": "blocked"}]})
            ),
            Merge::Blocked
        );
        assert_eq!(
            derive_merge(&serde_json::json!({"canMerge": true, "conflicted": false, "vetoes": []})),
            Merge::Clean
        );
        // canMerge:false with no vetoes (e.g. still computing) is not actionable → Clean.
        assert_eq!(
            derive_merge(&serde_json::json!({"canMerge": false, "conflicted": false, "vetoes": []})),
            Merge::Clean
        );
    }

    #[test]
    fn check_status_maps_every_documented_arm() {
        assert_eq!(check_status("SUCCESSFUL"), CheckStatus::Success);
        assert_eq!(check_status("FAILED"), CheckStatus::Failure);
        assert_eq!(check_status("INPROGRESS"), CheckStatus::Running);
        assert_eq!(check_status("CANCELLED"), CheckStatus::Skipped);
        assert_eq!(check_status("UNKNOWN"), CheckStatus::Pending);
        assert_eq!(check_status("anything-else"), CheckStatus::Pending);
    }

    #[test]
    fn epoch_ms_round_trips_through_parse_iso() {
        for ms in [0i64, 1_752_192_000_000, 4_102_444_799_000] {
            let iso = epoch_ms_to_iso(ms);
            assert_eq!(crate::forge::parse_iso(&iso), Some(ms / 1000));
        }
    }

    #[test]
    fn map_activities_maps_anchored_findings_general_comments_and_bots() {
        let activities = serde_json::json!([
            {
                "action": "COMMENTED",
                "commentAnchor": {"path": "src/a.rs", "line": 12, "orphaned": false},
                "comment": {
                    "text": "fix this", "state": "RESOLVED", "createdDate": 1_752_192_000_000i64,
                    "author": {"name": "persijano"},
                    "comments": [
                        {"text": "on it", "createdDate": 1_752_192_100_000i64, "author": {"name": "reviewer2"}},
                        {"text": "done", "createdDate": 1_752_192_200_000i64, "author": {"name": "persijano"}}
                    ]
                }
            },
            {
                "action": "COMMENTED",
                "commentAnchor": {"path": "src/b.rs", "orphaned": true},
                "comment": {
                    "text": "stale line", "createdDate": 1_752_192_300_000i64,
                    "author": {"name": "persijano"}
                }
            },
            {
                "action": "COMMENTED",
                "comment": {
                    "text": "looks good overall", "createdDate": 1_752_192_400_000i64,
                    "author": {"name": "reviewer2"}
                }
            },
            {
                "action": "COMMENTED",
                "comment": {
                    "text": "automated note", "createdDate": 1_752_192_500_000i64,
                    "author": {"name": "ci-runner", "type": "SERVICE"}
                }
            },
            {
                "action": "APPROVED",
                "comment": {"text": "should be dropped", "createdDate": 1_752_192_600_000i64, "author": {"name": "x"}}
            }
        ]);
        let cs = map_activities(&activities);
        assert_eq!(cs.len(), 4); // the non-COMMENTED activity is dropped

        let finding = cs.iter().find(|c| c.anchor == "src/a.rs:12").unwrap();
        assert_eq!(finding.kind, CommentKind::Finding);
        assert!(finding.is_resolved);
        assert!(!finding.is_outdated);
        assert_eq!(finding.reply_count, 2);

        let orphaned = cs.iter().find(|c| c.anchor == "src/b.rs").unwrap();
        assert!(orphaned.is_outdated);
        assert!(!orphaned.is_resolved);
        assert_eq!(orphaned.reply_count, 0);

        let plain = cs.iter().find(|c| c.author == "reviewer2").unwrap();
        assert_eq!(plain.kind, CommentKind::Comment);
        assert_eq!(plain.anchor, "comment");

        let bot = cs.iter().find(|c| c.author == "ci-runner").unwrap();
        assert!(bot.author_is_bot);

        // Newest first, matching the other backends' merged-comment ordering.
        assert!(cs.windows(2).all(|w| w[0].created_at >= w[1].created_at));
    }

    #[test]
    fn map_activities_falls_back_to_a_name_suffix_when_no_user_type_is_present() {
        let activities = serde_json::json!([
            {"action": "COMMENTED", "comment": {"text": "hi", "createdDate": 0, "author": {"name": "deploy_bot"}}},
            {"action": "COMMENTED", "comment": {"text": "hi", "createdDate": 0, "author": {"name": "release-bot"}}},
            {"action": "COMMENTED", "comment": {"text": "hi", "createdDate": 0, "author": {"name": "persijano"}}}
        ]);
        let cs = map_activities(&activities);
        assert!(cs.iter().find(|c| c.author == "deploy_bot").unwrap().author_is_bot);
        assert!(cs.iter().find(|c| c.author == "release-bot").unwrap().author_is_bot);
        assert!(!cs.iter().find(|c| c.author == "persijano").unwrap().author_is_bot);
    }

    #[test]
    fn map_activities_a_user_typed_normal_never_reads_as_a_bot_by_name_suffix() {
        // Once a `type` field is present it is authoritative — a real user whose account
        // happens to be named with a `-bot` suffix never misreads as a service account.
        let activities = serde_json::json!([
            {"action": "COMMENTED", "comment": {"text": "hi", "createdDate": 0,
                "author": {"name": "release-bot", "type": "NORMAL"}}}
        ]);
        let cs = map_activities(&activities);
        assert!(!cs[0].author_is_bot);
    }

    #[test]
    fn build_snapshot_detects_a_fork_by_differing_project_key_case_insensitively() {
        let detail = serde_json::json!({
            "id": 42, "title": "Add feature", "description": "Adds the thing.",
            "state": "OPEN", "draft": true,
            "links": {"self": [{"href": "https://bb.example.com/projects/TEAM/repos/svc/pull-requests/42"}]},
            "fromRef": {"displayId": "feat/x", "latestCommit": "abc123",
                        "repository": {"slug": "svc", "project": {"key": "team"}}},
            "toRef": {"displayId": "main", "repository": {"slug": "svc", "project": {"key": "TEAM"}}}
        });
        let s = build_snapshot(&detail, Merge::Clean, vec![], vec![], Sync::InSync, false);
        assert_eq!(s.number, 42);
        assert!(s.is_draft);
        assert_eq!(s.body, "Adds the thing.");
        assert_eq!(s.head_oid, "abc123");
        assert!(!s.head_is_fork); // "team" vs "TEAM" — same key, case-insensitive
        assert_eq!(s.head_ref, "feat/x");
        assert_eq!(s.base_ref, "main");
        assert_eq!(s.url, "https://bb.example.com/projects/TEAM/repos/svc/pull-requests/42");

        let mut forked = detail.clone();
        forked["fromRef"]["repository"]["project"]["key"] = serde_json::json!("OTHER");
        assert!(build_snapshot(&forked, Merge::Clean, vec![], vec![], Sync::InSync, false).head_is_fork);

        // Absent fields default rather than fail — a mid-rollout API response degrades soft.
        let bare = serde_json::json!({"id": 7});
        let s = build_snapshot(&bare, Merge::Clean, vec![], vec![], Sync::InSync, false);
        assert!(!s.is_draft);
        assert_eq!(s.head_ref, "");
        assert_eq!(s.body, "");
        assert!(!s.head_is_fork);
    }

    #[test]
    fn token_from_prefers_the_env_var_over_the_credential_helper() {
        assert_eq!(
            token_from(Some("env-token".to_string()), || Some("cred-pw".to_string())),
            Ok("env-token".to_string())
        );
        assert_eq!(token_from(None, || Some("cred-pw".to_string())), Ok("cred-pw".to_string()));
        assert_eq!(
            token_from(Some(String::new()), || Some("cred-pw".to_string())),
            Ok("cred-pw".to_string())
        );
        assert!(token_from(None, || None).is_err());
        assert!(token_from(Some(String::new()), || Some(String::new())).is_err());
    }

    #[test]
    fn token_from_never_calls_the_credential_helper_when_env_is_set() {
        let calls = std::cell::Cell::new(0);
        let result = token_from(Some("env-token".to_string()), || {
            calls.set(calls.get() + 1);
            Some("cred-pw".to_string())
        });
        assert_eq!(result, Ok("env-token".to_string()));
        assert_eq!(calls.get(), 0, "credential helper closure must not run when env wins");
    }

    #[test]
    fn classify_maps_curl_failures() {
        assert!(matches!(classify(SpawnFail::NotFound), BitbucketError::NoCurl));
        assert!(matches!(
            classify(SpawnFail::Failed { stderr: "HTTP 401 Unauthorized".to_string() }),
            BitbucketError::NotAuthed
        ));
        assert!(matches!(
            classify(SpawnFail::Failed { stderr: "HTTP 403 Forbidden".to_string() }),
            BitbucketError::NotAuthed
        ));
        assert!(matches!(
            classify(SpawnFail::Failed { stderr: "HTTP 500".to_string() }),
            BitbucketError::Other(message) if message == "HTTP 500"
        ));
        assert!(matches!(classify(SpawnFail::Cancelled), BitbucketError::Other(_)));
        assert!(matches!(classify(SpawnFail::Io("boom".to_string())), BitbucketError::Other(message) if message == "boom"));
    }

    #[test]
    fn is_truncated_reads_the_bitbucket_paging_flag() {
        assert!(!is_truncated(&serde_json::json!({"isLastPage": true})));
        assert!(is_truncated(&serde_json::json!({"isLastPage": false})));
        // A missing flag is not read as truncated.
        assert!(!is_truncated(&serde_json::json!({})));
    }
}
