use rustc_hir::def_id::DefId;
#[allow(unused)]
use rustc_middle::mir::{Body, Location, Statement, Terminator, TerminatorEdges, TerminatorKind};
use rustc_middle::ty::{Instance, TyCtxt, TypingEnv};

use crate::rtool_info;

pub struct LockDevTool<'tcx> {
    tcx: TyCtxt<'tcx>,
}

impl<'tcx> LockDevTool<'tcx> {
    pub fn new(tcx: TyCtxt<'tcx>) -> Self {
        Self { tcx }
    }

    pub fn start(&self) {
        for id in self.tcx.hir_free_items() {
            let item = self.tcx.hir_item(id);
            let did = item.owner_id.def_id.to_def_id();
            let attrs = self.tcx.get_all_attrs(did);
            for attr in attrs {
                rtool_info!("{:?}", attr);
            }
        }
    }
}
