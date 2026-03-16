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

fn phase_cargo_rtool() {
    rtool_trace!("Start phase cargo-rtool.");
    cargo_check::run();
}

fn phase_rustc_wrapper() {
    rtool_trace!("Launch cargo-rtool again triggered by cargo check.");

    let is_primary = env::var("CARGO_PRIMARY_PACKAGE").is_ok();
    let package_name = env::var("CARGO_PKG_NAME").unwrap_or_default();

    if is_primary {
        rtool_debug!("run rtool for package {}", package_name);
        run_rtool();
        return;
    }

    rtool_debug!("run rustc for package {}", package_name);
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
