#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_session;

use rtool::{
    RTOOL_DEFAULT_ARGS, RtoolCallback, rtool_error, rtool_info, rtool_trace, utils::log::init_log,
};
use rustc_session::EarlyDiagCtxt;
use rustc_session::config::ErrorOutputType;
use std::env;

fn run_complier(args: &mut Vec<String>, callback: &mut RtoolCallback) {
    // Finally, add the default flags all the way in the beginning, but after the binary name.
    args.splice(1..1, RTOOL_DEFAULT_ARGS.iter().map(ToString::to_string));

    let handler = EarlyDiagCtxt::new(ErrorOutputType::default());
    rustc_driver::init_rustc_env_logger(&handler);
    rustc_driver::install_ice_hook("bug_report_url", |_| ());

    rustc_driver::run_compiler(args, callback);
    rtool_trace!("The arg for compilation is {:?}", args);
}

enum ArgParserState {
    Ready,
    MirName,
    MirNameExact,
    OutPath,
}

fn main() {
    _ = init_log().inspect_err(|err| eprintln!("Failed to init log: {err}"));
    // Parse the arguments from env.
    let mut args = vec![];
    let mut compiler = RtoolCallback::default();
    let mut state = ArgParserState::Ready;
    for arg in env::args() {
        match state {
            ArgParserState::Ready => match arg.as_str() {
                "-allmir" => compiler.enable_show_all_mir(),
                "-lockdev" => compiler.enable_lockdev(),
                "-deadlock" => compiler.enable_deadlock(),
                "-mir" => state = ArgParserState::MirName,
                "-mirexact" => state = ArgParserState::MirNameExact,
                "-outpath" => state = ArgParserState::OutPath,
                _ => args.push(arg),
            },
            ArgParserState::MirName => {
                if arg.starts_with("-") {
                    rtool_error!("Invalid function name: {}", arg);
                    return;
                }
                compiler.enable_show_mir_fuzzy(arg);
                state = ArgParserState::Ready;
            }
            ArgParserState::MirNameExact => {
                if arg.starts_with("-") {
                    rtool_error!("Invalid function name: {}", arg);
                    return;
                }
                compiler.enable_show_mir_exact(arg);
                state = ArgParserState::Ready;
            }
            ArgParserState::OutPath => {
                if arg.starts_with("-") {
                    rtool_error!("Invalid output path: {}", arg);
                    return;
                }
                compiler.set_mir_output_file(arg);
                state = ArgParserState::Ready;
            }
        }
    }
    rtool_info!("Start analysis with Rtool.");
    rtool_trace!("rtool received arguments{:#?}", env::args());
    rtool_trace!("arguments to rustc: {:?}", &args);

    run_complier(&mut args, &mut compiler);
}
