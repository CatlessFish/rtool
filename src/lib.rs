#![feature(rustc_private)]
#![feature(box_patterns)]
#![feature(macro_metavar_expr_concat)]

pub mod analysis;
pub mod utils;
extern crate rustc_abi;
extern crate rustc_ast;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_hir_pretty;
extern crate rustc_index;
extern crate rustc_interface;
extern crate rustc_metadata;
extern crate rustc_middle;
extern crate rustc_public;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_target;
extern crate thin_vec;

use rustc_ast::ast;
use rustc_driver::{Callbacks, Compilation};
use rustc_interface::{
    Config,
    interface::{self, Compiler},
};
use rustc_middle::{ty::TyCtxt, util::Providers};
use rustc_session::search_paths::PathKind;
use std::path::PathBuf;
use std::sync::Arc;

use analysis::show_mir::ShowAllMir;

use crate::analysis::{deadlock::DeadlockDetector, dev::LockDevTool, show_mir::FindAndShowMir};

// Insert rustc arguments at the beginning of the argument list that rtool wants to be
// set per default, for maximal validation power.
pub static RTOOL_DEFAULT_ARGS: &[&str] = &["-Zalways-encode-mir", "-Zmir-opt-level=0"];

/// This is the data structure to handle rtool options as a rustc callback.

#[derive(Debug, Clone, Hash)]
pub struct RtoolCallback {
    show_all_mir: bool,
    lockdev: bool,
    deadlock: bool,
    show_mir_list: Vec<String>,
    show_mir_fuzzy_list: Vec<String>,
    show_mir_output_file: Option<String>,
}

#[allow(clippy::derivable_impls)]
impl Default for RtoolCallback {
    fn default() -> Self {
        Self {
            show_all_mir: false,
            lockdev: false,
            deadlock: false,
            show_mir_list: vec![],
            show_mir_fuzzy_list: vec![],
            show_mir_output_file: None,
        }
    }
}

impl Callbacks for RtoolCallback {
    fn config(&mut self, config: &mut Config) {
        config.override_queries = Some(|_, providers| {
            providers.extern_queries.used_crate_source = |tcx, cnum| {
                let mut providers = Providers::default();
                rustc_metadata::provide(&mut providers);
                let mut crate_source = (providers.extern_queries.used_crate_source)(tcx, cnum);
                // HACK: rustc will emit "crate ... required to be available in rlib format, but
                // was not found in this form" errors once we use `tcx.dependency_formats()` if
                // there's no rlib provided, so setting a dummy path here to workaround those errors.
                Arc::make_mut(&mut crate_source).rlib = Some((PathBuf::new(), PathKind::All));
                crate_source
            };
        });
    }
    fn after_crate_root_parsing(
        &mut self,
        _compiler: &interface::Compiler,
        _krate: &mut ast::Crate,
    ) -> Compilation {
        Compilation::Continue
    }
    fn after_analysis<'tcx>(&mut self, _compiler: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
        rtool_trace!("Execute after_analysis() of compiler callbacks");
        rustc_public::rustc_internal::run(tcx, || {
            start_analyzer(tcx, self.clone());
        })
        .expect("msg");
        rtool_trace!("analysis done");
        Compilation::Continue
    }
}

impl RtoolCallback {
    /// Enable mir display.
    pub fn enable_show_all_mir(&mut self) {
        self.show_all_mir = true;
    }

    /// Test if all_mir display is enabled.
    pub fn is_show_all_mir_enabled(&self) -> bool {
        self.show_all_mir
    }

    pub fn enable_lockdev(&mut self) {
        self.lockdev = true;
    }

    pub fn is_lockdev_enabled(&self) -> bool {
        self.lockdev
    }

    pub fn enable_deadlock(&mut self) {
        self.deadlock = true;
    }

    pub fn is_deadlock_enabled(&self) -> bool {
        self.deadlock
    }

    pub fn enable_show_mir_exact(&mut self, fn_name: String) {
        self.show_mir_list.push(fn_name);
    }

    pub fn enable_show_mir_fuzzy(&mut self, fn_name: String) {
        self.show_mir_fuzzy_list.push(fn_name);
    }

    pub fn is_find_mir_enabled(&self) -> bool {
        !self.show_mir_list.is_empty() || !self.show_mir_fuzzy_list.is_empty()
    }

    pub fn set_mir_output_file(&mut self, filename: String) {
        self.show_mir_output_file = Some(filename);
    }
}

/// Start the analysis with the features enabled.
pub fn start_analyzer(tcx: TyCtxt, callback: RtoolCallback) {
    if callback.is_show_all_mir_enabled() {
        ShowAllMir::new(tcx).start();
    }

    if callback.is_lockdev_enabled() {
        LockDevTool::new(tcx).start();
    }

    if callback.is_deadlock_enabled() {
        DeadlockDetector::new(tcx).run();
    }

    if callback.is_find_mir_enabled() {
        FindAndShowMir::new(
            tcx,
            &callback.show_mir_list,
            &callback.show_mir_fuzzy_list,
            callback.show_mir_output_file,
        )
        .start();
    }
}
