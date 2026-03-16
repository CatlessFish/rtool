#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_session;

use clap::Parser;
use rtool::cli;
use rtool::help;
use rtool::{
    RTOOL_DEFAULT_ARGS, RtoolCallback, rtool_debug, rtool_info, rtool_trace, utils::log::init_log,
};
use rustc_session::EarlyDiagCtxt;
use rustc_session::config::ErrorOutputType;
use std::env;

fn run_complier(callback: &mut RtoolCallback) {
    let mut args = env::args().collect::<Vec<_>>();
    args.extend(RTOOL_DEFAULT_ARGS.iter().map(ToString::to_string));
    rtool_trace!("Final arguments to rustc: {:?}", args);

    let handler = EarlyDiagCtxt::new(ErrorOutputType::default());
    rustc_driver::init_rustc_env_logger(&handler);
    rustc_driver::install_ice_hook("bug_report_url", |_| ());

    rustc_driver::run_compiler(&args, callback);
    rtool_trace!("The arg for compilation is {:?}", args);
}

#[derive(Parser, Debug, Clone)]
#[command(override_usage = help::styled_rtool_usage())]
#[command(version = help::RTOOL_VERSION)]
#[command(styles = help::RTOOL_STYLING)]
struct RtoolCli {
    #[command(flatten)]
    args: cli::RtoolArgs,
}

fn main() {
    _ = init_log().inspect_err(|err| eprintln!("Failed to init log: {err}"));

    let mut cli_args =
        shlex::split(env::var("RTOOLFLAGS").unwrap_or_default().as_str()).unwrap_or_default();
    rtool_debug!("RTOOLFLAGS = {:?}", cli_args);
    cli_args.insert(0, "rtool".to_owned());
    let cli = RtoolCli::parse_from(cli_args);

    let mut compiler = RtoolCallback::new(cli.args);
    rtool_info!("Start analysis with Rtool.");
    rtool_trace!("rtool received arguments{:#?}", env::args());
    run_complier(&mut compiler);
}
