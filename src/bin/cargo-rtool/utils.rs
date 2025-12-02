use crate::args;
use std::{
    env,
    path::PathBuf,
    process::{self, Command},
};

fn find_rtool() -> PathBuf {
    let mut path = args::current_exe_path().to_owned();
    path.set_file_name("rtool");
    path
}

pub fn run_cmd(mut cmd: Command) {
    rtool_trace!("Command is: {:?}.", cmd);
    match cmd.status() {
        Ok(status) => {
            if !status.success() {
                process::exit(status.code().unwrap());
            }
        }
        Err(err) => panic!("Error in running {:?} {}.", cmd, err),
    }
}

pub fn run_rustc() {
    let mut cmd = Command::new("rustc");
    cmd.args(args::skip2());
    run_cmd(cmd);
}

pub fn run_rtool() {
    let mut cmd = Command::new(find_rtool());
    cmd.args(args::skip2());
    let magic = env::var("rtool_ARGS").expect("Missing rtool_ARGS.");
    let rtool_args: Vec<String> =
        serde_json::from_str(&magic).expect("Failed to deserialize rtool_ARGS.");
    cmd.args(rtool_args);
    run_cmd(cmd);
}
