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
        let mut count = 0;
        let mut irq_api: Vec<DefId> = vec![];
        for lid in self.tcx.hir_body_owners() {
            let did = lid.to_def_id();
            let name = self.tcx.def_path_str(did);
            if name.contains("interrupt_enable") {
                rtool_info!("{}", name);
                irq_api.push(did.clone());
            }
            count += 1;
        }

        for lid in self.tcx.hir_body_owners() {
            let did = lid.to_def_id();
            // find callers
            if let Some(_other) = self.tcx.hir_body_const_context(lid) {
                continue;
            }
            if self.tcx.is_mir_available(did) {
                let body: &Body = self.tcx.optimized_mir(did);
                for (bb, _bb_data) in body.basic_blocks.iter_enumerated() {
                    let loc = body.terminator_loc(bb);
                    let terminator = body
                        .stmt_at(loc) // Either<&Statement, &Terminator>
                        .right() // Right should be Terminator
                        .unwrap();
                    if let TerminatorKind::Call { ref func, .. } = terminator.kind {
                        if let Some((callee_id, generics)) = func.const_fn_def() {
                            let ty_env = TypingEnv::post_analysis(self.tcx, did);
                            if let Ok(Some(instance)) =
                                Instance::try_resolve(self.tcx, ty_env, callee_id, generics)
                            {
                                let instance_id = instance.def_id();
                                if irq_api.contains(&instance_id) {
                                    rtool_info!(
                                        "{} calls {}",
                                        self.tcx.def_path_str(did),
                                        self.tcx.def_path_str(instance_id)
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        rtool_info!("{} body owners in total", count);
    }
}
