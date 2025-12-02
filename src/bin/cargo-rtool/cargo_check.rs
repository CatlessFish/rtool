use crate::args;
use cargo_metadata::camino::Utf8Path;
use rtool::utils::log::rtool_error_and_exit;
use std::{env, process::Command, time::Duration};
use wait_timeout::ChildExt;

mod workspace;

pub fn run() {
    match env::var("RTOOL_RECURSIVE")
        .ok()
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("none") | None => default_run(),
        Some("deep") => workspace::deep_run(),
        Some("shallow") => workspace::shallow_run(),
        _ => rtool_error_and_exit(
            "`recursive` should only accept one the values: none, shallow or deep.",
        ),
    }
}

fn cargo_check(dir: &Utf8Path) {
    // always clean before check due to outdated except `RTOOL_CLEAN` is false
    rtool_trace!("cargo clean in package folder {dir}");
    cargo_clean(dir, args::rtool_clean());

    rtool_trace!("cargo check in package folder {dir}");
    let [rtool_args, cargo_args] = args::rtool_and_cargo_args();
    rtool_trace!("rtool_args={rtool_args:?}\tcargo_args={cargo_args:?}");

    /*Here we prepare the cargo command as cargo check, which is similar to build, but much faster*/
    let mut cmd = Command::new("cargo");
    cmd.current_dir(dir);
    cmd.arg("check");

    /* set the target as a filter for phase_rustc_rtool */
    cmd.args(cargo_args);

    // Serialize the remaining args into a special environment variable.
    // This will be read by `phase_rustc_rtool` when we go to invoke
    // our actual target crate (the binary or the test we are running).

    cmd.env(
        "rtool_ARGS",
        serde_json::to_string(rtool_args).expect("Failed to serialize args."),
    );

    // Invoke actual cargo for the job, but with different flags.
    let cargo_rtool_path = args::current_exe_path();
    cmd.env("RUSTC_WRAPPER", cargo_rtool_path);

    rtool_trace!("Command is: {:?}.", cmd);

    let mut child = cmd.spawn().expect("Could not run cargo check.");
    match child
        .wait_timeout(Duration::from_secs(60 * 60)) // 1 hour timeout
        .expect("Failed to wait for subprocess.")
    {
        Some(status) => {
            if !status.success() {
                rtool_error_and_exit("Finished with non-zero exit code.");
            }
        }
        None => {
            child.kill().expect("Failed to kill subprocess.");
            child.wait().expect("Failed to wait for subprocess.");
            rtool_error_and_exit("Process killed due to timeout.");
        }
    };
}

fn cargo_clean(dir: &Utf8Path, really: bool) {
    if really {
        if let Err(err) = Command::new("cargo").arg("clean").current_dir(dir).output() {
            rtool_error_and_exit(format!("`cargo clean` exits unexpectedly:\n{err}"));
        }
    }
}

/// Just like running a cargo check in a folder.
fn default_run() {
    cargo_check(".".into());
}
