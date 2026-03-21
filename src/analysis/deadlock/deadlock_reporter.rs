use petgraph::graph::{EdgeIndex, NodeIndex};
use rustc_hir::def_id::DefId;
use rustc_middle::mir::Location;
use rustc_middle::ty::TyCtxt;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt::Write as _;

use crate::analysis::deadlock::types::{lock::LockInstance, *};
use crate::rtool_info;

pub struct DeadlockReporter<'tcx, 'a> {
    tcx: TyCtxt<'tcx>,
    graph: &'a LockDependencyGraph,
}

impl<'tcx, 'a> DeadlockReporter<'tcx, 'a> {
    pub fn new(tcx: TyCtxt<'tcx>, graph: &'a LockDependencyGraph) -> Self {
        Self { tcx, graph }
    }

    pub fn run(&mut self, no_group: bool, expansion: Option<&InterruptLocksiteExpansion>) {
        let self_cycle_nodes = self_cycle_node(self.graph);
        rtool_info!("Found {} self-cycle nodes", self_cycle_nodes.len());

        if no_group {
            let mut pairs: Vec<(NodeIndex, EdgeIndex)> = self_cycle_nodes.into_iter().collect();
            pairs.sort_by(|(na, ea), (nb, eb)| {
                cmp_lock_instance(&self.graph.graph[*na], &self.graph.graph[*nb])
                    .then_with(|| ea.index().cmp(&eb.index()))
            });
            for (node, edge) in pairs {
                let lock = &self.graph.graph[node];
                let edge_w = &self.graph.graph[edge];
                let (old_list, new_list) = expansion
                    .and_then(|m| m.get(lock))
                    .map(|e| {
                        (
                            sorted_callsites(self.tcx, &e.old_sites),
                            sorted_locksites(self.tcx, &e.new_lock_sites),
                        )
                    })
                    .unwrap_or_else(|| {
                        (
                            vec![edge_w.old_lock_site.site],
                            vec![edge_w.new_lock_site.clone()],
                        )
                    });
                let mut msg = format!("Possible Deadlock at: {} (--no-group)\n", lock);
                writeln!(
                    msg,
                    "Possible acquisition in task context ({} sites):",
                    old_list.len()
                )
                .unwrap();
                for (i, cs) in old_list.iter().enumerate() {
                    writeln!(
                        msg,
                        "  [{}] {}",
                        i + 1,
                        format_callsite_for_report(self.tcx, cs)
                    )
                    .unwrap();
                }
                writeln!(
                    msg,
                    "Possible acquisition in ISR context ({} sites):",
                    new_list.len()
                )
                .unwrap();
                for (i, ls) in new_list.iter().enumerate() {
                    writeln!(
                        msg,
                        "  [{}] {}",
                        i + 1,
                        format_callsite_for_report(self.tcx, &ls.site)
                    )
                    .unwrap();
                }
                rtool_info!("{}", msg.trim_end());
            }
            return;
        }

        for (node, edge) in self_cycle_nodes {
            let e = &self.graph.graph[edge];
            rtool_info!(
                "Possible Deadlock at: {}\n\tFirst acquired at {}\n\tthen aquired at {}\n\ttype {:?}",
                self.graph.graph[node],
                format_callsite_for_report(self.tcx, &e.old_lock_site.site),
                format_callsite_for_report(self.tcx, &e.new_lock_site.site),
                e.edge_type,
            );
        }
    }

    pub fn print_result(&self) {}
}

/// Def-path of the enclosing function, plus `span_to_diagnostic_string` when MIR maps to a real span.
fn format_callsite_for_report(tcx: TyCtxt<'_>, site: &CallSite) -> String {
    let path = tcx.def_path_str(site.caller_def_id);
    match mir_callsite_source_label(tcx, site.caller_def_id, site.location) {
        Some(loc) => format!("{path} @ {loc}"),
        None => path,
    }
}

fn mir_callsite_source_label(tcx: TyCtxt<'_>, def_id: DefId, loc: Location) -> Option<String> {
    let body = tcx.optimized_mir(def_id);
    let si = body.source_info(loc);
    let span = si.span;
    if span.is_dummy() {
        return None;
    }
    let sm = tcx.sess.source_map();
    Some(sm.span_to_diagnostic_string(span))
}

fn cmp_lock_instance(a: &LockInstance, b: &LockInstance) -> Ordering {
    format!("{a}").cmp(&format!("{b}"))
}

fn cmp_callsite(tcx: TyCtxt<'_>, a: &CallSite, b: &CallSite) -> Ordering {
    tcx.def_path_str(a.caller_def_id)
        .cmp(&tcx.def_path_str(b.caller_def_id))
        .then_with(|| a.location.block.index().cmp(&b.location.block.index()))
        .then_with(|| a.location.statement_index.cmp(&b.location.statement_index))
}

fn sorted_callsites(tcx: TyCtxt<'_>, sites: &HashSet<CallSite>) -> Vec<CallSite> {
    let mut v: Vec<CallSite> = sites.iter().copied().collect();
    v.sort_by(|a, b| cmp_callsite(tcx, a, b));
    v
}

fn sorted_locksites(tcx: TyCtxt<'_>, sites: &HashSet<LockSite>) -> Vec<LockSite> {
    let mut v: Vec<LockSite> = sites.iter().cloned().collect();
    v.sort_by(|a, b| {
        cmp_lock_instance(&a.lock, &b.lock).then_with(|| cmp_callsite(tcx, &a.site, &b.site))
    });
    v
}

fn self_cycle_node(graph: &LockDependencyGraph) -> HashSet<(NodeIndex, EdgeIndex)> {
    let mut result: HashSet<(NodeIndex, EdgeIndex)> = HashSet::new();
    for edge_idx in graph.graph.edge_indices() {
        if let LockDependencyEdgeType::Call(_) = graph.graph[edge_idx].edge_type {
            // Temporarily only look for interrupt self cycle
            continue;
        }
        if let Some((start_node, end_node)) = graph.graph.edge_endpoints(edge_idx) {
            if start_node == end_node {
                result.insert((start_node, edge_idx));
            }
        }
    }
    result
}
