/*
    This is a cargo program to start rtool.
    The file references the cargo file for Miri: https://github.com/rust-lang/miri/blob/master/cargo-miri/src/main.rs
*/
#![feature(rustc_private)]

#[macro_use]
extern crate rtool;

use clap::Parser;
use rtool::cli;
use rtool::help;
use rtool::utils::log::{init_log, rtool_error_and_exit};
use std::env;

mod args;
mod utils;
use crate::utils::*;

mod cargo_check;

fn rustc_arg_value(flag: &str) -> Option<String> {
    let args = args::skip2();
    let prefix = format!("{flag}=");
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        if arg == flag {
            return iter.next().cloned();
        }
        if let Some(value) = arg.strip_prefix(&prefix) {
            return Some(value.to_string());
        }
    }

    None
}

fn rustc_arg_values(flag: &str) -> Vec<String> {
    let args = args::skip2();
    let prefix = format!("{flag}=");
    let mut values = Vec::new();
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        if arg == flag {
            if let Some(value) = iter.next() {
                values.push(value.clone());
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix(&prefix) {
            values.push(value.to_string());
        }
    }

    values
}

fn should_run_rtool(is_primary: bool) -> bool {
    if !is_primary {
        return false;
    }

    let crate_name = rustc_arg_value("--crate-name");
    let crate_types = rustc_arg_values("--crate-type");
    let target = rustc_arg_value("--target");

    let is_build_script = crate_name.as_deref() == Some("build_script_build");
    let is_proc_macro = crate_types.iter().any(|ty| ty == "proc-macro");
    let is_target_unit = target.is_some();

    rtool_debug!(
        "rustc wrapper filter: crate_name={:?}, crate_types={:?}, target={:?}, is_primary={}, is_build_script={}, is_proc_macro={}",
        crate_name,
        crate_types,
        target,
        is_primary,
        is_build_script,
        is_proc_macro
    );

    is_target_unit && !is_build_script && !is_proc_macro
}

fn phase_cargo_rtool() {
    rtool_trace!("Start phase cargo-rtool.");
    cargo_check::run();
}

fn phase_rustc_wrapper() {
    rtool_trace!("Launch cargo-rtool again triggered by cargo check.");

    let is_primary = env::var("CARGO_PRIMARY_PACKAGE").is_ok();
    let package_name = env::var("CARGO_PKG_NAME").unwrap_or_default();
    let crate_name = rustc_arg_value("--crate-name").unwrap_or_default();

    if should_run_rtool(is_primary) {
        rtool_debug!(
            "run rtool for package {} crate {}",
            package_name,
            crate_name
        );
        run_rtool();
        return;
    }

    rtool_debug!(
        "run rustc for package {} crate {}",
        package_name,
        crate_name
    );
    run_rustc();
}

#[derive(Parser, Debug)]
#[command(name = "cargo")]
#[command(bin_name = "cargo")]
#[command(version, about)]
#[command(styles = help::CARGO_RTOOL_STYLING)]
enum CargoCli {
    #[command(override_usage = help::styled_cargo_rtool_usage())]
    #[command(version = help::RTOOL_VERSION)]
    #[command(after_help = help::RTOOL_AFTER_HELP)]
    Rtool(cli::RtoolArgs),
}

impl CargoCli {
    fn args(&self) -> &cli::RtoolArgs {
        match self {
            CargoCli::Rtool(args) => args,
        }
    }
}

fn main() {
    /* This function will be enteredd twice:
       1. When we run `cargo rtool ...`, cargo dispatches the execution to cargo-rtool.
      In this step, we set RUSTC_WRAPPER to cargo-rtool, and execute `cargo check ...` command;
       2. Cargo check actually triggers `path/cargo-rtool path/rustc` according to RUSTC_WRAPPER.
          Because RUSTC_WRAPPER is defined, Cargo calls the command: `$RUSTC_WRAPPER path/rustc ...`
    */

    // Init the log_system
    init_log().expect("Failed to init log.");

    match args::get_arg(1).unwrap() {
        "rtool" => {
            let _ = args::cargo_cli();
            phase_cargo_rtool()
        }
        arg if arg.ends_with("rustc") => phase_rustc_wrapper(),
        _ => rtool_error_and_exit(
            "rtool must be called with either `rtool` or `rustc` as first argument.",
        ),
    }
}
