pub const RTOOL_HELP: &str = r#"
Usage:
    cargo rtool [rtool options] -- [cargo check options]

rtool Options:

Analysis:
    -allmir             show mir of every fn
    -mir fn_name        show mir with def_path_str containing with fn_name
    -mirexact fn_name   show mir with def_path_str = fn_name

General command: 
    -help:     show help information
    -version:  show the version of rtool

NOTE: multiple detections can be processed in single run by 
appending the options to the arguments.

Environment Variables (Values are case insensitive):
    RTOOL_LOG          verbosity of logging: trace, debug, info, warn
                     trace: print all the detailed rtool execution traces.
                     debug: display intermidiate analysis results.
                     warn: show bugs detected only.

    RTOOL_CLEAN        run cargo clean before check: true, false
                     * true is the default value except that false is set

    RTOOL_RECURSIVE    scope of packages to check: none, shallow, deep
                     * none or the variable not set: check for current folder
                     * shallow: check for current workpace members
                     * deep: check for all workspaces from current folder
                      
                     NOTE: for shallow or deep, rtool will enter each member
                     folder to do the check.
"#;

pub const RTOOL_VERSION: &str = r#"
rtool version 0.1
"#;
