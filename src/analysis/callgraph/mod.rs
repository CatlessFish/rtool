pub mod default;
pub mod visitor;

use crate::analysis::Analysis;
use rustc_hir::def_id::DefId;
use rustc_middle::ty::TyCtxt;
use std::{collections::HashMap, fmt};

/// This is the data structure used to store function calls.
/// It contains a HashMap that records the callees of all functions.
pub struct CallGraph {
    pub fn_calls: HashMap<DefId, Vec<DefId>>, // caller_id -> Vec<(callee_id)>
}

impl CallGraph {
    pub fn new() -> Self {
        Self {
            fn_calls: HashMap::new(),
        }
    }

    pub fn get_callees(&self, caller_def_id: DefId) -> Vec<DefId> {
        self.fn_calls
            .get(&caller_def_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_callees_recursive(&self, caller_def_id: DefId) -> Vec<DefId> {
        let mut visited = std::collections::HashSet::new();
        let mut result = Vec::new();
        let mut stack = vec![caller_def_id];
        while let Some(current) = stack.pop() {
            if let Some(callees) = self.fn_calls.get(&current) {
                for callee in callees {
                    if visited.insert(*callee) {
                        result.push(*callee);
                        stack.push(*callee);
                    }
                }
            }
        }
        result
    }
}

pub struct CallGraphDisplay<'a, 'tcx> {
    pub graph: &'a CallGraph,
    pub tcx: TyCtxt<'tcx>,
}

impl<'a, 'tcx> fmt::Display for CallGraphDisplay<'a, 'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "CallGraph:")?;
        for (caller, callees) in &self.graph.fn_calls {
            let caller_name = self.tcx.def_path_str(*caller);
            writeln!(f, "  {} calls:", caller_name)?;
            for callee in callees {
                let callee_name = self.tcx.def_path_str(*callee);
                writeln!(f, "    -> {}", callee_name)?;
            }
        }
        Ok(())
    }
}

/// This trait provides features related to call graph extraction and analysis.
pub trait CallGraphAnalysis: Analysis {
    /// Return the call graph.
    fn get_callgraph(&mut self) -> CallGraph;
}
