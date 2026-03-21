use clap::builder::{
    Styles,
    styling::{Effects, Style},
};

pub const RTOOL_AFTER_HELP: &str = r#"Examples:
  cargo rtool analyze deadlock
  cargo rtool analyze deadlock --save-tags tags.json
  cargo rtool analyze deadlock --load-tags tags.json
  cargo rtool analyze dev
  cargo rtool analyze mir --all
  cargo rtool analyze mir --exact crate::path::foo --outpath mir.txt
  cargo rtool analyze callchain --from crate::path::foo --to crate::path::bar
  cargo rtool analyze callchain --from foo --to bar --all-paths --outpath callchain.txt
  cargo rtool analyze mir --fuzzy foo -- --tests

Environment Variables (values are case insensitive):
  RTOOL_LOG        verbosity of logging: trace, debug, info, warn
  RTOOL_CLEAN      run cargo clean before check: true, false
  RTOOL_RECURSIVE  scope of packages to check: none, shallow, deep
"#;

pub const RTOOL_VERSION: &str = concat!("version ", env!("CARGO_PKG_VERSION"));

pub const CARGO_RTOOL_STYLING: Styles = clap_cargo::style::CLAP_STYLING;
pub const RTOOL_STYLING: Styles = clap_cargo::style::CLAP_STYLING;

pub fn styled_str(s: &str, style: &Style, bold: bool) -> String {
    let style = if bold {
        style.effects(Effects::BOLD)
    } else {
        *style
    };
    format!("\x1b[{}{}\x1b[0m", style.render(), s)
}

pub fn styled_cargo_rtool_usage() -> String {
    let style = CARGO_RTOOL_STYLING.get_literal();
    format!(
        "{} {}",
        styled_str("cargo rtool", &style, true),
        styled_str("[OPTIONS] <COMMAND> [-- [CARGO_FLAGS]]", &style, false)
    )
}

pub fn styled_rtool_usage() -> String {
    let style = RTOOL_STYLING.get_literal();
    format!(
        "{} {} {}",
        styled_str("RTOOLFLAGS=\"[OPTIONS] <COMMAND>\"", &style, false),
        styled_str("rtool", &style, true),
        styled_str("[RUSTFLAGS]", &style, false)
    )
}
