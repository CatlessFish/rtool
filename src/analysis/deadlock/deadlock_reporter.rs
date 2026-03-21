use petgraph::Direction;
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
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
        rtool_info!(
            "Found {} single-lock interrupt self-cycle edge(s)",
            self_cycle_nodes.len()
        );

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
        } else {
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

        let two_lock_pairs = collect_two_lock_reciprocal_pairs(self.graph);
        rtool_info!("Found {} two-lock reciprocal pair(s)", two_lock_pairs.len());

        for (lock_a, lock_b) in two_lock_pairs {
            let Some(na) = node_index_for_lock(self.graph, &lock_a) else {
                continue;
            };
            let Some(nb) = node_index_for_lock(self.graph, &lock_b) else {
                continue;
            };

            let mut msg = format!("Possible deadlock (two locks): {} <-> {}\n", lock_a, lock_b);

            if no_group {
                append_direction_report_no_group(
                    self.tcx, self.graph, &mut msg, &lock_a, &lock_b, na, nb,
                );
                append_direction_report_no_group(
                    self.tcx, self.graph, &mut msg, &lock_b, &lock_a, nb, na,
                );
            } else {
                append_direction_report_grouped(
                    self.tcx, self.graph, &mut msg, &lock_a, &lock_b, na, nb,
                );
                append_direction_report_grouped(
                    self.tcx, self.graph, &mut msg, &lock_b, &lock_a, nb, na,
                );
            }

            rtool_info!("{}", msg.trim_end());
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

fn node_index_for_lock(graph: &LockDependencyGraph, lock: &LockInstance) -> Option<NodeIndex> {
    graph
        .graph
        .node_indices()
        .find(|&n| &graph.graph[n] == lock)
}

/// Unordered distinct lock pairs with both A->B and B->A edges in the LDG.
fn collect_two_lock_reciprocal_pairs(
    graph: &LockDependencyGraph,
) -> Vec<(LockInstance, LockInstance)> {
    let g = &graph.graph;
    let mut seen: HashSet<(LockInstance, LockInstance)> = HashSet::new();

    for edge_idx in g.edge_indices() {
        let Some((src, dst)) = g.edge_endpoints(edge_idx) else {
            continue;
        };
        if src == dst {
            continue;
        }
        let has_reverse = g
            .edges_directed(dst, Direction::Outgoing)
            .any(|e| e.target() == src);
        if !has_reverse {
            continue;
        }

        let lu = g[src].clone();
        let lv = g[dst].clone();
        if lu == lv {
            continue;
        }

        let (a, b) = match cmp_lock_instance(&lu, &lv) {
            Ordering::Less => (lu, lv),
            Ordering::Greater => (lv, lu),
            Ordering::Equal => continue,
        };
        seen.insert((a, b));
    }

    let mut v: Vec<(LockInstance, LockInstance)> = seen.into_iter().collect();
    v.sort_by(|(a1, b1), (a2, b2)| {
        cmp_lock_instance(a1, a2).then_with(|| cmp_lock_instance(b1, b2))
    });
    v
}

fn edge_indices_src_to_dst(
    graph: &LockDependencyGraph,
    src: NodeIndex,
    dst: NodeIndex,
) -> Vec<EdgeIndex> {
    let g = &graph.graph;
    let mut v: Vec<EdgeIndex> = g
        .edges_directed(src, Direction::Outgoing)
        .filter(|e| e.target() == dst)
        .map(|e| e.id())
        .collect();
    v.sort_by_key(|e| e.index());
    v
}

fn append_direction_report_grouped(
    tcx: TyCtxt<'_>,
    graph: &LockDependencyGraph,
    msg: &mut String,
    src_lock: &LockInstance,
    dst_lock: &LockInstance,
    src_node: NodeIndex,
    dst_node: NodeIndex,
) {
    let edges = edge_indices_src_to_dst(graph, src_node, dst_node);
    let mut first_call: Option<EdgeIndex> = None;
    let mut first_interrupt: Option<EdgeIndex> = None;
    for eid in &edges {
        match &graph.graph[*eid].edge_type {
            LockDependencyEdgeType::Call(_) => {
                if first_call.is_none() {
                    first_call = Some(*eid);
                }
            }
            LockDependencyEdgeType::Interrupt(_) => {
                if first_interrupt.is_none() {
                    first_interrupt = Some(*eid);
                }
            }
        }
    }

    writeln!(msg, "Dependency {} -> {}:", src_lock, dst_lock).unwrap();
    if let Some(eid) = first_call {
        let e = &graph.graph[eid];
        writeln!(
            msg,
            "  Call edge: acquire {} at {}; while holding {} at {}",
            src_lock,
            format_callsite_for_report(tcx, &e.new_lock_site.site),
            dst_lock,
            format_callsite_for_report(tcx, &e.old_lock_site.site),
        )
        .unwrap();
    }
    if let Some(eid) = first_interrupt {
        let e = &graph.graph[eid];
        writeln!(
            msg,
            "  Interrupt edge: acquire {} at {}; while holding {} at {}",
            src_lock,
            format_callsite_for_report(tcx, &e.new_lock_site.site),
            dst_lock,
            format_callsite_for_report(tcx, &e.old_lock_site.site),
        )
        .unwrap();
    }
    if first_call.is_none() && first_interrupt.is_none() {
        writeln!(msg, "  (no edges)").unwrap();
    }
}

fn append_direction_report_no_group(
    tcx: TyCtxt<'_>,
    graph: &LockDependencyGraph,
    msg: &mut String,
    src_lock: &LockInstance,
    dst_lock: &LockInstance,
    src_node: NodeIndex,
    dst_node: NodeIndex,
) {
    let mut new_sites: HashSet<CallSite> = HashSet::new();
    let mut old_sites: HashSet<CallSite> = HashSet::new();
    for eid in edge_indices_src_to_dst(graph, src_node, dst_node) {
        let e = &graph.graph[eid];
        new_sites.insert(e.new_lock_site.site);
        old_sites.insert(e.old_lock_site.site);
    }

    let new_list = sorted_callsites(tcx, &new_sites);
    let old_list = sorted_callsites(tcx, &old_sites);

    writeln!(msg, "Dependency {} -> {} (--no-group)", src_lock, dst_lock).unwrap();
    writeln!(
        msg,
        "Possible acquisition of lock {} ({} sites):",
        src_lock,
        new_list.len()
    )
    .unwrap();
    for (i, cs) in new_list.iter().enumerate() {
        writeln!(msg, "  [{}] {}", i + 1, format_callsite_for_report(tcx, cs)).unwrap();
    }
    writeln!(
        msg,
        "While holding lock {} ({} sites):",
        dst_lock,
        old_list.len()
    )
    .unwrap();
    for (i, cs) in old_list.iter().enumerate() {
        writeln!(msg, "  [{}] {}", i + 1, format_callsite_for_report(tcx, cs)).unwrap();
    }
}
