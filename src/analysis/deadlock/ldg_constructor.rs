use petgraph::dot::{Config, Dot};
use petgraph::visit::{EdgeRef, IntoNodeReferences};
use rustc_hir::BodyOwnerKind;
use rustc_hir::def_id::DefId;
use rustc_middle::mir::Body;
use rustc_middle::mir::visit::Visitor;
use rustc_middle::ty::TyCtxt;
use std::collections::{HashMap, HashSet};

use crate::analysis::deadlock::types::{interrupt::*, lock::*, *};
use crate::rtool_info;

type LockSitePairsWithCallSite = Vec<(LockSite, LockSite, CallSite)>;

struct InterruptEdgeCollector<'tcx, 'a> {
    tcx: TyCtxt<'tcx>,
    func_def_id: DefId,
    program_lock_set: &'a ProgramLockSet,
    program_isr_info: &'a ProgramIsrInfo,
    isr_lock_ops_by_lock: &'a HashMap<LockInstance, LockSite>,
    locksite_pairs: LockSitePairsWithCallSite,
    seen_locks: HashSet<LockInstance>,
}

impl<'tcx, 'a> InterruptEdgeCollector<'tcx, 'a> {
    pub fn new(
        tcx: TyCtxt<'tcx>,
        func_def_id: DefId,
        program_lock_set: &'a ProgramLockSet,
        program_isr_info: &'a ProgramIsrInfo,
        isr_lock_ops_by_lock: &'a HashMap<LockInstance, LockSite>,
    ) -> Self {
        Self {
            tcx,
            func_def_id,
            program_lock_set,
            program_isr_info,
            isr_lock_ops_by_lock,
            locksite_pairs: Vec::new(),
            seen_locks: HashSet::new(),
        }
    }

    pub fn collect(mut self) -> LockSitePairsWithCallSite {
        let body: &Body = self.tcx.optimized_mir(self.func_def_id);
        self.visit_body(body);
        self.locksite_pairs
    }
}

impl<'tcx, 'a> Visitor<'tcx> for InterruptEdgeCollector<'tcx, 'a> {
    fn visit_terminator(
        &mut self,
        _terminator: &rustc_middle::mir::Terminator<'tcx>,
        location: rustc_middle::mir::Location,
    ) {
        let irq_state = match self.program_isr_info.func_irq_infos.get(&self.func_def_id) {
            Some(func_info) => func_info.pre_bb_irq_states.get(&location.block).unwrap(),
            None => return,
        };
        if *irq_state == IrqState::MustBeDisabled {
            return;
        }

        let callsite_lockset = match self.program_lock_set.get(&self.func_def_id) {
            Some(func_info) => func_info.pre_bb_locksets.get(&location.block).unwrap(),
            None => return,
        };

        for (held_lock, state) in callsite_lockset.lock_states.iter() {
            if *state != LockState::MayHold {
                continue;
            }

            if !self.seen_locks.insert(held_lock.clone()) {
                continue;
            }

            let Some(held_callsite) = callsite_lockset
                .lock_sites
                .get(held_lock)
                .and_then(|sites| sites.iter().next().copied())
            else {
                continue;
            };

            let Some(new_lock_site) = self.isr_lock_ops_by_lock.get(held_lock) else {
                continue;
            };

            self.locksite_pairs.push((
                new_lock_site.clone(),
                LockSite {
                    lock: held_lock.clone(),
                    site: held_callsite,
                },
                CallSite {
                    caller_def_id: self.func_def_id,
                    location,
                },
            ));
        }
    }
}

/// All ISR lock operations per lock instance (for expanded reporting / second pass).
pub fn collect_all_isr_lock_ops_by_lock(
    program_lock_set: &ProgramLockSet,
    program_isr_info: &ProgramIsrInfo,
) -> HashMap<LockInstance, HashSet<LockSite>> {
    let mut map: HashMap<LockInstance, HashSet<LockSite>> = HashMap::new();

    for isr_def_id in program_isr_info.isr_funcs.iter() {
        let Some(func_info) = program_lock_set.get(isr_def_id) else {
            continue;
        };

        for lock_site in func_info.lock_operations.iter() {
            map.entry(lock_site.lock.clone())
                .or_default()
                .insert(lock_site.clone());
        }
    }

    map
}

struct ExpandedInterruptEdgeCollector<'tcx, 'a> {
    tcx: TyCtxt<'tcx>,
    func_def_id: DefId,
    program_lock_set: &'a ProgramLockSet,
    program_isr_info: &'a ProgramIsrInfo,
    isr_lock_ops_by_lock: &'a HashMap<LockInstance, HashSet<LockSite>>,
    expansion: &'a mut InterruptLocksiteExpansion,
}

impl<'tcx, 'a> ExpandedInterruptEdgeCollector<'tcx, 'a> {
    fn new(
        tcx: TyCtxt<'tcx>,
        func_def_id: DefId,
        program_lock_set: &'a ProgramLockSet,
        program_isr_info: &'a ProgramIsrInfo,
        isr_lock_ops_by_lock: &'a HashMap<LockInstance, HashSet<LockSite>>,
        expansion: &'a mut InterruptLocksiteExpansion,
    ) -> Self {
        Self {
            tcx,
            func_def_id,
            program_lock_set,
            program_isr_info,
            isr_lock_ops_by_lock,
            expansion,
        }
    }

    fn collect(mut self) {
        let body: &Body = self.tcx.optimized_mir(self.func_def_id);
        self.visit_body(body);
    }
}

impl<'tcx, 'a> Visitor<'tcx> for ExpandedInterruptEdgeCollector<'tcx, 'a> {
    fn visit_terminator(
        &mut self,
        _terminator: &rustc_middle::mir::Terminator<'tcx>,
        location: rustc_middle::mir::Location,
    ) {
        let irq_state = match self.program_isr_info.func_irq_infos.get(&self.func_def_id) {
            Some(func_info) => func_info.pre_bb_irq_states.get(&location.block).unwrap(),
            None => return,
        };
        if *irq_state == IrqState::MustBeDisabled {
            return;
        }

        let callsite_lockset = match self.program_lock_set.get(&self.func_def_id) {
            Some(func_info) => func_info.pre_bb_locksets.get(&location.block).unwrap(),
            None => return,
        };

        for (held_lock, state) in callsite_lockset.lock_states.iter() {
            if *state != LockState::MayHold {
                continue;
            }

            let Some(old_sites) = callsite_lockset.lock_sites.get(held_lock) else {
                continue;
            };
            if old_sites.is_empty() {
                continue;
            }
            let Some(isr_sites) = self.isr_lock_ops_by_lock.get(held_lock) else {
                continue;
            };
            if isr_sites.is_empty() {
                continue;
            }

            let entry = self.expansion.entry(held_lock.clone()).or_default();
            for held_callsite in old_sites.iter().copied() {
                entry.old_sites.insert(held_callsite);
            }
            for new_lock_site in isr_sites.iter().cloned() {
                entry.new_lock_sites.insert(new_lock_site);
            }
        }
    }
}

/// Second pass: all task/ISR locksites that can participate in an interrupt self-edge for each lock.
pub fn collect_interrupt_locksite_expansion<'tcx>(
    tcx: TyCtxt<'tcx>,
    program_lock_set: &ProgramLockSet,
    program_isr_info: &ProgramIsrInfo,
) -> InterruptLocksiteExpansion {
    let isr_map = collect_all_isr_lock_ops_by_lock(program_lock_set, program_isr_info);
    let mut expansion: InterruptLocksiteExpansion = HashMap::new();

    for local_def_id in tcx.hir_body_owners() {
        let def_id = match tcx.hir_body_owner_kind(local_def_id) {
            BodyOwnerKind::Fn => local_def_id.to_def_id(),
            _ => continue,
        };
        ExpandedInterruptEdgeCollector::new(
            tcx,
            def_id,
            program_lock_set,
            program_isr_info,
            &isr_map,
            &mut expansion,
        )
        .collect();
    }

    expansion
}

pub struct LDGConstructor<'tcx, 'a> {
    tcx: TyCtxt<'tcx>,
    program_lock_set: &'a ProgramLockSet,
    program_isr_info: &'a ProgramIsrInfo,
    isr_lock_ops_by_lock: HashMap<LockInstance, LockSite>,
    graph: LockDependencyGraph,
}

impl<'tcx, 'a> LDGConstructor<'tcx, 'a> {
    pub fn new(
        tcx: TyCtxt<'tcx>,
        program_lock_set: &'a ProgramLockSet,
        program_isr_info: &'a ProgramIsrInfo,
    ) -> Self {
        Self {
            tcx,
            program_isr_info,
            program_lock_set,
            isr_lock_ops_by_lock: collect_isr_lock_ops_by_lock(program_lock_set, program_isr_info),
            graph: LockDependencyGraph::new(),
        }
    }

    pub fn run(&mut self) {
        for local_def_id in self.tcx.hir_body_owners() {
            let def_id = match self.tcx.hir_body_owner_kind(local_def_id) {
                BodyOwnerKind::Fn => local_def_id.to_def_id(),
                _ => continue,
            };
            let intr_edges = InterruptEdgeCollector::new(
                self.tcx,
                def_id,
                self.program_lock_set,
                self.program_isr_info,
                &self.isr_lock_ops_by_lock,
            )
            .collect();

            for (new, old, callsite) in intr_edges.iter() {
                self.graph.insert_interrupt_edge(new, old, callsite);
            }
        }
    }

    pub fn print_result(&self) {
        let mut result = String::new();
        result.push('\n');
        for (idx, lock) in self.graph.graph.node_references() {
            result.push_str(format!("{} {}\n", idx.index(), lock).as_str());
        }
        let mut call_edge_num = 0;
        let mut intr_edge_num = 0;
        for edge in self.graph.graph.edge_references() {
            result.push_str(
                format!(
                    "{} -> {} | {}\n",
                    edge.source().index(),
                    edge.target().index(),
                    edge.weight()
                )
                .as_str(),
            );
            if let LockDependencyEdgeType::Call(_) = edge.weight().edge_type {
                call_edge_num += 1;
            } else {
                intr_edge_num += 1;
            }
        }
        result.push_str(
            format!(
                "{} call edges, {} intr edges\n",
                call_edge_num, intr_edge_num
            )
            .as_str(),
        );
        rtool_info!("{}", result);
    }

    pub fn print_dot_graph(&self) {
        rtool_info!(
            "\n{:?}",
            Dot::with_config(&self.graph.graph, &[Config::GraphContentOnly])
        );
    }

    pub fn into_graph(self) -> LockDependencyGraph {
        self.graph
    }
}

fn collect_isr_lock_ops_by_lock(
    program_lock_set: &ProgramLockSet,
    program_isr_info: &ProgramIsrInfo,
) -> HashMap<LockInstance, LockSite> {
    let mut isr_lock_ops_by_lock = HashMap::new();

    for isr_def_id in program_isr_info.isr_funcs.iter() {
        let Some(func_info) = program_lock_set.get(isr_def_id) else {
            continue;
        };

        for lock_site in func_info.lock_operations.iter() {
            isr_lock_ops_by_lock
                .entry(lock_site.lock.clone())
                .or_insert_with(|| lock_site.clone());
        }
    }

    isr_lock_ops_by_lock
}
