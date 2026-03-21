#![feature(rustc_private)]
#![feature(box_patterns)]
#![feature(macro_metavar_expr_concat)]

#[macro_use]
pub mod utils;
pub mod analysis;
pub mod cli;
pub mod help;
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
use std::path::PathBuf;
use std::sync::Arc;

use analysis::callchain::CallChainAnalyzer;
use analysis::show_mir::ShowAllMir;

use crate::{
    analysis::{deadlock::DeadlockDetector, dev::LockDevTool, show_mir::FindAndShowMir},
    cli::{AnalysisKind, Commands, RtoolArgs},
};

// Insert rustc arguments at the beginning of the argument list that rtool wants to be
// set per default, for maximal validation power.
pub static RTOOL_DEFAULT_ARGS: &[&str] = &["-Zalways-encode-mir", "-Zmir-opt-level=0"];

/// This is the data structure to handle rtool options as a rustc callback.

#[derive(Debug, Clone)]
pub struct RtoolCallback {
    args: RtoolArgs,
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
                Arc::make_mut(&mut crate_source).rlib = Some(PathBuf::new());
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
    pub fn new(args: RtoolArgs) -> Self {
        Self { args }
    }
}

/// Start the analysis with the features enabled.
pub fn start_analyzer(tcx: TyCtxt, callback: RtoolCallback) {
    match &callback.args.command {
        Commands::Analyze { kind } => match kind {
            AnalysisKind::Deadlock {
                save_tags,
                load_tags,
                no_group,
            } => {
                DeadlockDetector::new(tcx).run_with_tag_io(
                    save_tags.as_deref(),
                    load_tags.as_deref(),
                    *no_group,
                );
            }
            AnalysisKind::Dev => {
                LockDevTool::new(tcx).start();
            }
            AnalysisKind::Mir {
                all,
                exact,
                fuzzy,
                outpath,
            } => {
                if *all || (exact.is_empty() && fuzzy.is_empty()) {
                    ShowAllMir::new(tcx).start();
                } else {
                    FindAndShowMir::new(tcx, exact, fuzzy, outpath.clone()).start();
                }
            }
            AnalysisKind::Callchain {
                from,
                to,
                all_paths,
                outpath,
            } => {
                CallChainAnalyzer::new(tcx, from, to, *all_paths, outpath.clone()).start();
            }
        },
    }
}
