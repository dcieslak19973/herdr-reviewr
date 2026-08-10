#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn reviewr_bin() -> &'static str {
    env!("CARGO_BIN_EXE_herdr-reviewr")
}

/// A fake herdr, answering in the live 0.7.5 envelope shapes (docs/herdr-api-notes.md).
/// `pane list` serves `panes.json` (else one plain pane), `pane process-info` serves the
/// per-pane `procinfo-<id>.json` (else a plain shell, which is not a reviewr pane) or fails
/// with `procfail-<id>.json` on stderr, `pane close` succeeds unless `closefail-<id>`
/// exists (whose content becomes the failure's stderr), `plugin config-dir` names the
/// fixture dir itself (after a 5s hang when `configdir-hang` exists), and everything else
/// answers as a successful `plugin pane open`.
fn fake_herdr(dir: &Path) -> (PathBuf, PathBuf) {
    let path = dir.join("herdr");
    let log = dir.join("herdr.log");
    fs::write(
        &path,
        format!(
            concat!(
                "#!/bin/sh\n",
                "dir='{dir}'\n",
                "printf '%s\\n' \"$*\" >> '{log}'\n",
                "case \"$*\" in\n",
                "  'pane list'*)\n",
                "    if [ -f \"$dir/panes.json\" ]; then cat \"$dir/panes.json\";\n",
                "    else printf '%s\\n' '{{\"result\":{{\"panes\":[{{\"pane_id\":\"w1:p1\"}}]}}}}'; fi ;;\n",
                "  'pane process-info --pane '*)\n",
                "    if [ -f \"$dir/procfail-$4.json\" ]; then cat \"$dir/procfail-$4.json\" >&2; exit 1; fi\n",
                "    if [ -f \"$dir/procinfo-$4.json\" ]; then cat \"$dir/procinfo-$4.json\";\n",
                "    else printf '%s\\n' '{{\"result\":{{\"process_info\":{{\"foreground_process_group_id\":7,\"foreground_processes\":[{{\"pid\":7,\"name\":\"zsh\",\"argv0\":\"zsh\",\"argv\":[\"-zsh\"],\"cwd\":\"/\"}}],\"pane_id\":\"'\"$4\"'\",\"shell_pid\":1}}}}}}'; fi ;;\n",
                "  'pane close '*)\n",
                "    if [ -f \"$dir/closefail-$3\" ]; then cat \"$dir/closefail-$3\" >&2; exit 1; fi\n",
                "    printf '%s\\n' '{{\"result\":{{}}}}' ;;\n",
                "  'plugin config-dir '*)\n",
                "    if [ -f \"$dir/configdir-hang\" ]; then sleep 5; fi\n",
                "    printf '%s\\n' \"$dir\" ;;\n",
                "  *) printf '%s\\n' '{{\"result\":{{\"plugin_pane\":{{\"pane\":{{\"pane_id\":\"w1:p9\",\"tab_id\":\"w1:t9\"}}}}}}}}' ;;\n",
                "esac\n",
            ),
            dir = dir.display(),
            log = log.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    (path, log)
}

/// One `pane process-info` answer for `pane`: a foreground process group holding `entries`,
/// in the live envelope shape (docs/herdr-api-notes.md).
fn procinfo(dir: &Path, pane: &str, entries: &str) {
    fs::write(
        dir.join(format!("procinfo-{pane}.json")),
        format!(
            r#"{{"result":{{"process_info":{{"foreground_process_group_id":7,"foreground_processes":[{entries}],"pane_id":"{pane}","shell_pid":1}}}}}}"#
        ),
    )
    .unwrap();
}

fn run(mode: &str, config_dir: &Path, herdr: &Path) -> Output {
    Command::new("bash")
        .arg("herdr/pane.sh")
        .arg(mode)
        .env("HERDR_REVIEWR_BIN", reviewr_bin())
        .env("HERDR_PLUGIN_CONFIG_DIR", config_dir)
        .env("HERDR_BIN_PATH", herdr)
        .env("HERDR_WORKSPACE_ID", "workspace-1")
        .output()
        .unwrap()
}

/// An `open` with the workspace context a focused pane provides, so the run reaches the
/// placement and `plugin pane open` stages.
fn run_open(config_dir: &Path, herdr: &Path) -> Output {
    let context = serde_json::json!({"focused_pane_cwd": env!("CARGO_MANIFEST_DIR")}).to_string();
    Command::new("bash")
        .arg("herdr/pane.sh")
        .arg("open")
        .env("HERDR_REVIEWR_BIN", reviewr_bin())
        .env("HERDR_PLUGIN_CONFIG_DIR", config_dir)
        .env("HERDR_BIN_PATH", herdr)
        .env("HERDR_WORKSPACE_ID", "workspace-1")
        .env("HERDR_PANE_ID", "w1:p1")
        .env("HERDR_PLUGIN_CONTEXT_JSON", &context)
        .output()
        .unwrap()
}

#[test]
fn invalid_config_refuses_manual_action_before_herdr_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "theme = \"not-a-theme\"\n").unwrap();
    let (herdr, log) = fake_herdr(dir.path());

    for mode in ["open", "close", "toggle"] {
        let output = run(mode, dir.path(), &herdr);
        assert_eq!(output.status.code(), Some(1), "{mode}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("config.toml"), "{mode}: {stderr}");
        assert!(stderr.contains("`theme`"), "{mode}: {stderr}");
    }
    assert!(!log.exists(), "herdr was invoked before validation");
}

#[test]
fn invalid_config_refuses_event_loudly_before_herdr_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "auto_open = \"sometimes\"\n").unwrap();
    let (herdr, log) = fake_herdr(dir.path());

    let output = run("auto-open", dir.path(), &herdr);

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("`auto_open`"));
    assert!(!log.exists(), "herdr was invoked before validation");
}

#[test]
fn corrected_config_recovers_on_the_next_invocation() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    fs::write(&config, "unknown = true\n").unwrap();
    let (herdr, log) = fake_herdr(dir.path());
    assert_eq!(run("close", dir.path(), &herdr).status.code(), Some(1));
    assert!(!log.exists());

    fs::write(&config, "theme = \"gruvbox\"\n").unwrap();
    let output = run("close", dir.path(), &herdr);

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("close: nothing open"));
    assert!(fs::read_to_string(log).unwrap().contains("pane list --workspace workspace-1"));
}

#[test]
fn disabled_auto_open_stops_after_successful_validation() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "auto_open = false\n").unwrap();
    let (herdr, log) = fake_herdr(dir.path());

    let output = run("auto-open", dir.path(), &herdr);

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(!log.exists());
}

#[test]
fn valid_auto_open_runtime_refusal_remains_silent() {
    let dir = tempfile::tempdir().unwrap();
    let (herdr, log) = fake_herdr(dir.path());

    let output = Command::new("bash")
        .arg("herdr/pane.sh")
        .arg("auto-open")
        .env("HERDR_REVIEWR_BIN", reviewr_bin())
        .env("HERDR_PLUGIN_CONFIG_DIR", dir.path())
        .env("HERDR_BIN_PATH", &herdr)
        .env_remove("HERDR_WORKSPACE_ID")
        .env_remove("HERDR_PANE_ID")
        .env_remove("HERDR_PLUGIN_CONTEXT_JSON")
        .env_remove("HERDR_PLUGIN_EVENT_JSON")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(!log.exists());
}

// --- Pane identity (specs/herdr-host.md): the foreground process decides, never the label.

#[test]
fn a_pane_running_the_review_ui_counts_however_it_was_launched() {
    let dir = tempfile::tempdir().unwrap();
    let (herdr, log) = fake_herdr(dir.path());
    // A wrapped launch: `cargo run` holds the group, its child is the review UI, and the
    // pane carries no `reviewr` label at all (HH-LAUNCHER-BLIND).
    procinfo(
        dir.path(),
        "w1:p1",
        concat!(
            r#"{"pid":7,"name":"cargo","argv0":"cargo","argv":["cargo","run"],"cwd":"/w"},"#,
            // The child's title (`name`) is rewritten, so only the executable identifies it.
            r#"{"pid":8,"name":"some-title","argv0":"target/debug/herdr-reviewr","argv":["target/debug/herdr-reviewr"],"cwd":"/w"}"#
        ),
    );

    let output = run("open", dir.path(), &herdr);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("already open (w1:p1)"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !fs::read_to_string(&log).unwrap().contains("plugin pane open"),
        "an open over a live pane must not stack another"
    );

    // `close` sweeps the same pane by the same live read, with a plain `pane close`.
    let output = run("close", dir.path(), &herdr);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("closed w1:p1"));
    let calls = fs::read_to_string(&log).unwrap();
    assert!(calls.lines().any(|l| l == "pane close w1:p1"), "{calls}");
}

#[test]
fn close_sweeps_every_reviewr_pane_and_a_close_that_lost_the_race_still_converges() {
    let dir = tempfile::tempdir().unwrap();
    let (herdr, log) = fake_herdr(dir.path());
    // w1:p2 is a plain shell wearing a stale `reviewr` label — a crashed binary's leftover.
    // The label is display only and never read (specs/herdr-host.md, Pane identity), so the
    // sweep below must not touch it.
    fs::write(
        dir.path().join("panes.json"),
        r#"{"result":{"panes":[{"pane_id":"w1:p1"},{"pane_id":"w1:p2","label":"reviewr"},{"pane_id":"w1:p3"}]}}"#,
    )
    .unwrap();
    let ui = r#"{"pid":8,"name":"herdr-reviewr","argv0":"herdr-reviewr","argv":["/plugin/bin/herdr-reviewr"],"cwd":"/w"}"#;
    procinfo(dir.path(), "w1:p1", ui);
    procinfo(dir.path(), "w1:p3", ui);
    // w1:p3's close fails with the pane gone: it exited between the read and the close.
    // The sweep still exits 0 — the end state is the same (specs/herdr-host.md, Failure
    // semantics).
    fs::write(
        dir.path().join("closefail-w1:p3"),
        r#"{"error":{"code":"pane_not_found","message":"pane w1:p3 not found"},"id":"cli:request"}"#,
    )
    .unwrap();

    let output = run("close", dir.path(), &herdr);

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("closed w1:p1 w1:p3"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let calls = fs::read_to_string(&log).unwrap();
    // Whole log lines, so a `plugin pane close` could not satisfy the plain-`pane close`
    // contract these assert (specs/herdr-host.md, Failure semantics).
    assert!(calls.lines().any(|l| l == "pane close w1:p1"), "{calls}");
    assert!(calls.lines().any(|l| l == "pane close w1:p3"), "{calls}");
    assert!(
        !calls.contains("pane close w1:p2"),
        "a labeled plain shell must not be swept: {calls}"
    );
}

#[test]
fn a_close_that_fails_for_a_live_pane_sweeps_the_rest_then_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let (herdr, log) = fake_herdr(dir.path());
    fs::write(
        dir.path().join("panes.json"),
        r#"{"result":{"panes":[{"pane_id":"w1:p1"},{"pane_id":"w1:p3"}]}}"#,
    )
    .unwrap();
    let ui = r#"{"pid":8,"name":"herdr-reviewr","argv0":"herdr-reviewr","argv":["/plugin/bin/herdr-reviewr"],"cwd":"/w"}"#;
    procinfo(dir.path(), "w1:p1", ui);
    procinfo(dir.path(), "w1:p3", ui);
    // w1:p1's close fails with the pane still there — a wedged herdr, not the benign
    // exited-between-read-and-close race. Reporting it closed would leave a running pane
    // the user believes gone, so the sweep refuses (specs/herdr-host.md, Failure semantics).
    fs::write(
        dir.path().join("closefail-w1:p1"),
        r#"{"error":{"code":"internal","message":"boom"},"id":"cli:request"}"#,
    )
    .unwrap();

    let output = run("close", dir.path(), &herdr);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("pane close failed for w1:p1"), "{stderr}");
    // The refusal comes after the sweep, so the panes herdr could close are closed.
    let calls = fs::read_to_string(&log).unwrap();
    assert!(calls.lines().any(|l| l == "pane close w1:p3"), "{calls}");
}

#[test]
fn a_gone_pane_skips_and_an_unreadable_read_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let (herdr, _log) = fake_herdr(dir.path());
    // The read reports the pane gone: it exited between the list and the read, so the
    // action converges — this close has nothing to sweep and exits 0.
    fs::write(
        dir.path().join("procfail-w1:p1.json"),
        r#"{"error":{"code":"pane_not_found","message":"pane w1:p1 not found"},"id":"cli:request"}"#,
    )
    .unwrap();
    let output = run("close", dir.path(), &herdr);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("close: nothing open"));

    // Any other read failure refuses, never reads as "no reviewr pane": an open would
    // stack a duplicate and a close would false-succeed (specs/herdr-host.md).
    fs::write(
        dir.path().join("procfail-w1:p1.json"),
        r#"{"error":{"code":"internal","message":"boom"},"id":"cli:request"}"#,
    )
    .unwrap();
    for mode in ["open", "close", "toggle"] {
        let output = run(mode, dir.path(), &herdr);
        assert_eq!(output.status.code(), Some(1), "{mode}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("process-info failed"),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn an_action_repoints_the_stable_launch_paths_at_the_live_plugin_root() {
    let dir = tempfile::tempdir().unwrap();
    let (herdr, _log) = fake_herdr(dir.path());
    // The install's build step runs in a staging checkout herdr renames afterwards, so the
    // actions own the stable links: every valid invocation re-points them at the runtime
    // root (specs/herdr-host.md, Install paths). `~/.local/bin` only when it exists.
    let home = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("bin")).unwrap();
    fs::write(root.path().join("bin/herdr-reviewr"), "#!/bin/sh\n").unwrap();
    let mut permissions =
        fs::metadata(root.path().join("bin/herdr-reviewr")).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(root.path().join("bin/herdr-reviewr"), permissions).unwrap();
    let run_close = |home: &Path| {
        Command::new("bash")
            .arg("herdr/pane.sh")
            .arg("close")
            .env("HERDR_REVIEWR_BIN", reviewr_bin())
            .env("HERDR_PLUGIN_CONFIG_DIR", dir.path())
            .env("HERDR_BIN_PATH", &herdr)
            .env("HERDR_WORKSPACE_ID", "workspace-1")
            .env("HERDR_PLUGIN_ROOT", root.path())
            .env("HOME", home)
            .output()
            .unwrap()
    };

    let output = run_close(home.path());

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let state_link =
        home.path().join(".local/state/herdr/plugins/persiyanov.reviewr/bin/herdr-reviewr");
    assert_eq!(fs::read_link(&state_link).unwrap(), root.path().join("bin/herdr-reviewr"));
    let bin_link = home.path().join(".local/bin/herdr-reviewr");
    assert!(!bin_link.exists(), "~/.local/bin must not be created for the link");

    // With `~/.local/bin` present, the second link lands too — and an existing symlink
    // re-points rather than blocks.
    fs::create_dir_all(home.path().join(".local/bin")).unwrap();
    std::os::unix::fs::symlink("/nonexistent/old", &bin_link).unwrap();
    let output = run_close(home.path());
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(fs::read_link(&bin_link).unwrap(), root.path().join("bin/herdr-reviewr"));
}

#[test]
fn a_process_info_answer_missing_its_shape_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let (herdr, _log) = fake_herdr(dir.path());
    // Exit 0 with an error envelope — no `.result.process_info.foreground_processes`. A
    // shape failure must refuse like a failed pane list, never read as "no reviewr pane".
    fs::write(
        dir.path().join("procinfo-w1:p1.json"),
        r#"{"error":{"code":"internal","message":"boom"},"id":"cli:request"}"#,
    )
    .unwrap();
    for mode in ["open", "close", "toggle"] {
        let output = run(mode, dir.path(), &herdr);
        assert_eq!(output.status.code(), Some(1), "{mode}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("process-info failed"),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn a_failed_pane_list_refuses_rather_than_reading_as_no_pane() {
    let dir = tempfile::tempdir().unwrap();
    let (herdr, _log) = fake_herdr(dir.path());
    // `pane list` answering an error envelope (exit 0, no `.result.panes`) must refuse:
    // read as "no reviewr pane", an open would stack a duplicate and a close would
    // false-succeed with panes still running.
    fs::write(
        dir.path().join("panes.json"),
        r#"{"error":{"code":"internal","message":"boom"},"id":"cli:request"}"#,
    )
    .unwrap();

    let output = run("close", dir.path(), &herdr);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("pane list failed"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_cli_fallback_resolves_the_config_dir_when_the_env_names_none() {
    // The launcher-blind half of config resolution (specs/config.md): with no
    // `HERDR_PLUGIN_CONFIG_DIR`, the binary asks `herdr plugin config-dir` and reads the
    // directory it names. This is the one test that exercises the real herdr-CLI path —
    // the unit tests drive the resolver with an injected closure.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "theme = \"gruvbox\"\n").unwrap();
    let (herdr, _log) = fake_herdr(dir.path());

    let output = Command::new(reviewr_bin())
        .arg("--resolve-plugin-config")
        .env_remove("HERDR_PLUGIN_CONFIG_DIR")
        .env("HERDR_BIN_PATH", &herdr)
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("gruvbox"), "expected the CLI-named dir's config: {stdout}");
}

#[test]
fn a_wedged_config_dir_lookup_degrades_to_the_defaults_inside_the_bound() {
    // A herdr that does not answer resolves no directory, the missing-file outcome
    // (specs/config.md, Failure semantics). The fake hangs 5s, well past the binary's
    // bound, so a success here can only come from giving the lookup up.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "theme = \"gruvbox\"\n").unwrap();
    fs::write(dir.path().join("configdir-hang"), "").unwrap();
    let (herdr, _log) = fake_herdr(dir.path());

    let output = Command::new(reviewr_bin())
        .arg("--resolve-plugin-config")
        .env_remove("HERDR_PLUGIN_CONFIG_DIR")
        .env("HERDR_BIN_PATH", &herdr)
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("gruvbox"), "a hung lookup must name no directory: {stdout}");
    assert!(stdout.contains("\"theme\""), "the defaults still print in full: {stdout}");
}

#[test]
fn a_flag_run_never_counts_as_the_review_ui() {
    let dir = tempfile::tempdir().unwrap();
    let (herdr, log) = fake_herdr(dir.path());
    // The review binary run for its config flag is not the review UI (Pane identity), so
    // `open` opens a fresh pane over it. The fake's default answer — a plain shell — is the
    // not-a-reviewr-pane baseline every placement test below relies on.
    procinfo(
        dir.path(),
        "w1:p1",
        r#"{"pid":7,"name":"herdr-reviewr","argv0":"herdr-reviewr","argv":["herdr-reviewr","--resolve-plugin-config"],"cwd":"/w"}"#,
    );

    let output = run_open(dir.path(), &herdr);

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let calls = fs::read_to_string(&log).unwrap();
    assert!(calls.contains("plugin pane open"), "a flag run must not read as open: {calls}");
}

#[test]
fn the_flag_dispatch_matches_the_actions_anywhere_in_argv() {
    // The other half of the flag-run contract, pinned in the binary itself: `pane.sh`
    // excludes `--resolve-plugin-config` wherever it sits in argv, so `main.rs` must
    // recognize it there too — or a flag run would start the review UI while the actions
    // refuse to count it (specs/herdr-host.md, Pane identity).
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "theme = \"gruvbox\"\n").unwrap();

    let output = Command::new(reviewr_bin())
        .args(["--some-future-arg", "--resolve-plugin-config"])
        .env("HERDR_PLUGIN_CONFIG_DIR", dir.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"theme\""), "expected resolved config JSON, got: {stdout}");
}

#[test]
fn valid_non_default_placement_and_direction_reach_herdr_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let (herdr, log) = fake_herdr(dir.path());

    let cases = [
        ("toggle_placement = \"overlay\"\n", "--placement overlay", None),
        (
            "toggle_placement = \"split\"\ntoggle_direction = \"down\"\n",
            "--placement split",
            Some("--direction down"),
        ),
    ];
    for (text, placement, direction) in cases {
        fs::write(&config, text).unwrap();
        let _ = fs::remove_file(&log);
        let output = run_open(dir.path(), &herdr);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let calls = fs::read_to_string(&log).unwrap();
        assert!(calls.contains(placement), "{calls}");
        if let Some(direction) = direction {
            assert!(calls.contains(direction), "{calls}");
        }
    }
}

#[test]
fn tab_placement_open_names_its_fresh_tab() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "toggle_placement = \"tab\"\n").unwrap();
    let (herdr, log) = fake_herdr(dir.path());

    let output = run_open(dir.path(), &herdr);

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let calls = fs::read_to_string(&log).unwrap();
    assert!(calls.contains("tab rename w1:t9 reviewr"), "{calls}");
}

#[test]
fn split_placement_open_renames_no_tab() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "toggle_placement = \"split\"\n").unwrap();
    let (herdr, log) = fake_herdr(dir.path());

    let output = run_open(dir.path(), &herdr);

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let calls = fs::read_to_string(&log).unwrap();
    assert!(!calls.contains("tab rename"), "{calls}");
}
