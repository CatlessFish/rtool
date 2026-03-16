use clap::Parser;
use std::{
    env,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use crate::CargoCli;

struct Arguments {
    args: Vec<String>,
    rtool_args: Vec<String>,
    cargo_args: Vec<String>,
    current_exe_path: PathBuf,
    rtool_clean: bool,
}

impl Arguments {
    fn new() -> Self {
        fn rtool_clean() -> bool {
            match env::var("RTOOL_CLEAN")
                .ok()
                .map(|s| s.trim().to_ascii_lowercase())
                .as_deref()
            {
                Some("false") => false,
                _ => true, // clean is the preferred behavior
            }
        }

        let args: Vec<_> = env::args().collect();
        let path = env::current_exe().expect("Current executable path invalid.");
        rtool_trace!("Current exe: {path:?}\tReceived args: {args:?}");
        let [args_group1, args_group2] = split_args_by_double_dash(&args);

        Arguments {
            args,
            rtool_args: args_group1,
            cargo_args: args_group2,
            current_exe_path: path,
            rtool_clean: rtool_clean(),
        }
    }
}

pub fn rtool_clean() -> bool {
    ARGS.rtool_clean
}

fn split_args_by_double_dash(args: &[String]) -> [Vec<String>; 2] {
    let mut args = args.iter().skip(2).map(|arg| arg.to_owned());
    let rtool_args = args.by_ref().take_while(|arg| *arg != "--").collect();
    let cargo_args = args.collect();
    [rtool_args, cargo_args]
}

static ARGS: LazyLock<Arguments> = LazyLock::new(Arguments::new);
static CLI: LazyLock<CargoCli> =
    LazyLock::new(|| CargoCli::parse_from(env::args().take_while(|arg| *arg != "--")));

pub fn cargo_cli() -> &'static CargoCli {
    &CLI
}

/// `cargo rtool [rtool options] -- [cargo check options]`
///
/// Options before the first `--` are arguments forwarding to rtool.
/// Stuff all after the first `--` are arguments forwarding to cargo check.
pub fn rtool_and_cargo_args() -> [&'static [String]; 2] {
    [&ARGS.rtool_args, &ARGS.cargo_args]
}

pub fn get_arg(pos: usize) -> Option<&'static str> {
    ARGS.args.get(pos).map(|x| x.as_str())
}

pub fn skip2() -> &'static [String] {
    ARGS.args.get(2..).unwrap_or(&[])
}

pub fn current_exe_path() -> &'static Path {
    &ARGS.current_exe_path
}
