/*
    This is a cargo program to start rtool.
    The file references the cargo file for Miri: https://github.com/rust-lang/miri/blob/master/cargo-miri/src/main.rs
*/
#![feature(rustc_private)]

#[macro_use]
extern crate rtool;

use rtool::utils::log::{init_log, rtool_error_and_exit};

mod args;
mod help;

mod utils;
use crate::utils::*;

mod cargo_check;

fn phase_cargo_rtool() {
    rtool_trace!("Start cargo-rtool.");

    // here we skip two args: cargo rtool
    let Some(arg) = args::get_arg(2) else {
        rtool_error!("Expect command: e.g., `cargo rtool -help`.");
        return;
    };
    match arg {
        "-version" => {
            rtool_info!("{}", help::RTOOL_VERSION);
            return;
        }
        "-help" => {
            rtool_info!("{}", help::RTOOL_HELP);
            return;
        }
        _ => {}
    }

    cargo_check::run();
}

fn phase_rustc_wrapper() {
    rtool_trace!("Launch cargo-rtool again triggered by cargo check.");

    let is_direct = args::is_current_compile_crate();
    // rtool only checks local crates
    if is_direct && args::filter_crate_type() {
        run_rtool();
        return;
    }

    // for dependencies and some special crate types, run rustc as usual
    run_rustc();
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
        s if s.ends_with("rtool") => phase_cargo_rtool(),
        s if s.ends_with("rustc") => phase_rustc_wrapper(),
        _ => rtool_error_and_exit(
            "rtool must be called with either `rtool` or `rustc` as first argument.",
        ),
    }
}
