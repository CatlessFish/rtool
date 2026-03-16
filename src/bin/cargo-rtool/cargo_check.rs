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
    let [rtool_args, cargo_args] = args::rtool_and_cargo_args();
    let timeout = args::cargo_cli().args().timeout;

    cargo_clean(dir, args::rtool_clean());
    rtool_trace!("cargo check in package folder {dir}");
    rtool_trace!("rtool_args={rtool_args:?}\tcargo_args={cargo_args:?}");

    /*Here we prepare the cargo command as cargo check, which is similar to build, but much faster*/
    let mut cmd = Command::new("cargo");
    cmd.current_dir(dir).arg("check");

    /* set the target as a filter for phase_rustc_rtool */
    cmd.args(cargo_args);
    cmd.env("RTOOLFLAGS", rtool_args.join(" "));

    // Invoke actual cargo for the job, but with different flags.
    let cargo_rtool_path = args::current_exe_path();
    cmd.env("RUSTC_WRAPPER", cargo_rtool_path);

    rtool_trace!("Command is: {:?}.", cmd);

    let mut child = cmd.spawn().expect("Could not run cargo check.");
    if let Some(timeout) = timeout {
        match child
            .wait_timeout(Duration::from_secs(timeout))
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
    } else if !child.wait().unwrap().success() {
        rtool_error_and_exit("Finished with non-zero exit code.");
    }

    cargo_clean(dir, args::rtool_clean());
}

fn cargo_clean(dir: &Utf8Path, really: bool) {
    if really {
        rtool_trace!("cargo clean in package folder {dir}");
        if let Err(err) = Command::new("cargo")
            .arg("clean")
            .arg("--workspace")
            .current_dir(dir)
            .output()
        {
            rtool_error_and_exit(format!("`cargo clean` exits unexpectedly:\n{err}"));
        }
    }
}

/// Just like running a cargo check in a folder.
fn default_run() {
    cargo_check(".".into());
}
