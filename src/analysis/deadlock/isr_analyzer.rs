use rustc_hir::def_id::DefId;
use rustc_middle::mir::{
    Body, Local, Location, Statement, Terminator, TerminatorEdges, TerminatorKind,
};
use rustc_middle::ty::TyCtxt;
use std::collections::{HashMap, HashSet};

extern crate rustc_mir_dataflow;
use rustc_mir_dataflow::fmt::DebugWithContext;
use rustc_mir_dataflow::{Analysis, JoinSemiLattice};

use crate::analysis::callgraph::CallGraph;
use crate::analysis::deadlock::tag_parser::LockTagItem;
use crate::analysis::deadlock::types::{interrupt::*, lock::*};
use crate::{rtool_debug, rtool_info};

#[derive(Debug, Clone, PartialEq, Eq)]
struct IrqAnalysisState {
    ambient: IrqState,
    active_irq_disabled_guards: HashSet<Local>,
}

impl IrqAnalysisState {
    fn new() -> Self {
        Self {
            ambient: IrqState::new(),
            active_irq_disabled_guards: HashSet::new(),
        }
    }

    fn effective_irq_state(&self) -> IrqState {
        if !self.active_irq_disabled_guards.is_empty() {
            IrqState::MustBeDisabled
        } else {
            self.ambient.clone()
        }
    }
}

impl JoinSemiLattice for IrqAnalysisState {
    fn join(&mut self, other: &Self) -> bool {
        let old = self.clone();

        match (&self.ambient, &other.ambient) {
            (IrqState::Bottom, _) => *self = other.clone(),
            (_, IrqState::Bottom) => {}
            _ => {
                self.ambient = self.ambient.union(&other.ambient);
                self.active_irq_disabled_guards = self
                    .active_irq_disabled_guards
                    .intersection(&other.active_irq_disabled_guards)
                    .copied()
                    .collect();
            }
        }

        *self != old
    }
}

impl<C> DebugWithContext<C> for IrqAnalysisState {}

struct FuncIsrAnalyzer<'tcx, 'a> {
    tcx: TyCtxt<'tcx>,
    enable_interrupt_apis: Vec<DefId>,
    disable_interrupt_apis: Vec<DefId>,
    lock_ops: HashMap<DefId, bool>,
    lockmap: &'a LocalLockMap,
    analyzed_functions: &'a HashMap<DefId, FuncIrqInfo>,
}

impl<'tcx, 'a> FuncIsrAnalyzer<'tcx, 'a> {
    pub fn new(
        tcx: TyCtxt<'tcx>,
        enable_interrupt_apis: Vec<DefId>,
        disable_interrupt_apis: Vec<DefId>,
        lock_ops: HashMap<DefId, bool>,
        lockmap: &'a LocalLockMap,
        analyzed_functions: &'a HashMap<DefId, FuncIrqInfo>,
    ) -> Self {
        FuncIsrAnalyzer {
            tcx,
            enable_interrupt_apis,
            disable_interrupt_apis,
            lock_ops,
            lockmap,
            analyzed_functions,
        }
    }
}

impl<'tcx, 'a> Analysis<'tcx> for FuncIsrAnalyzer<'tcx, 'a> {
    type Domain = IrqAnalysisState;

    const NAME: &'static str = "ISRAnalysis";

    fn bottom_value(&self, _body: &Body<'tcx>) -> Self::Domain {
        IrqAnalysisState::new()
    }

    fn initialize_start_block(
        &self,
        _body: &rustc_middle::mir::Body<'tcx>,
        state: &mut Self::Domain,
    ) {
        *state = IrqAnalysisState::new()
    }

    fn apply_primary_statement_effect(
        &self,
        _state: &mut Self::Domain,
        _statement: &Statement<'tcx>,
        _location: Location,
    ) {
    }

    fn apply_primary_terminator_effect<'air>(
        &self,
        state: &mut Self::Domain,
        terminator: &'air Terminator<'tcx>,
        _location: Location,
    ) -> TerminatorEdges<'air, 'tcx> {
        match &terminator.kind {
            TerminatorKind::Call {
                func, destination, ..
            } => {
                if let Some(callee_def_id) = func.const_fn_def() {
                    if self.enable_interrupt_apis.contains(&callee_def_id.0) {
                        state.ambient = IrqState::MayBeEnabled;
                        return terminator.edges();
                    }

                    if self.disable_interrupt_apis.contains(&callee_def_id.0) {
                        state.ambient = IrqState::MustBeDisabled;
                        return terminator.edges();
                    }

                    if let Some(guard_irq_disabled) = self.lock_ops.get(&callee_def_id.0) {
                        if *guard_irq_disabled {
                            state.active_irq_disabled_guards.insert(destination.local);
                        }
                        return terminator.edges();
                    }

                    if self.tcx.is_mir_available(callee_def_id.0) {
                        if let Some(instance) = self
                            .analyzed_functions
                            .get(&callee_def_id.0)
                            .map(|callee_info| &callee_info.exit_irq_state)
                        {
                            state.ambient = state.ambient.union(instance);
                        }
                    }
                }
            }
            TerminatorKind::Drop { place, .. } => {
                if self.lockmap.get(&place.local).is_some_and(|infos| {
                    infos
                        .iter()
                        .any(|info| info.irq_semantics == GuardIrqSemantics::DisabledWhileHeld)
                }) {
                    state.active_irq_disabled_guards.remove(&place.local);
                }
            }
            _ => {}
        }
        terminator.edges()
    }
}

pub struct IsrAnalyzer<'tcx, 'a> {
    tcx: TyCtxt<'tcx>,
    callgraph: &'a CallGraph,
    parsed_tags: &'a [LockTagItem],
    program_lock_info: &'a ProgramLockInfo,
    enable_interrupt_apis: Vec<DefId>,
    disable_interrupt_apis: Vec<DefId>,
    lock_ops: HashMap<DefId, bool>,
    program_isr_info: ProgramIsrInfo,
}

impl<'tcx, 'a> IsrAnalyzer<'tcx, 'a> {
    pub fn new(
        tcx: TyCtxt<'tcx>,
        callgraph: &'a CallGraph,
        parsed_tags: &'a [LockTagItem],
        program_lock_info: &'a ProgramLockInfo,
    ) -> Self {
        Self {
            tcx,
            callgraph,
            parsed_tags,
            program_lock_info,
            enable_interrupt_apis: vec![],
            disable_interrupt_apis: vec![],
            lock_ops: HashMap::new(),
            program_isr_info: ProgramIsrInfo::new(),
        }
    }

    pub fn run(&mut self) -> ProgramIsrInfo {
        self.collect_isr();
        self.collect_interrupt_apis();
        self.analyze_interrupt_set();

        rtool_info!("Collected {} ISRs", self.program_isr_info.isr_funcs.len());
        self.program_isr_info.clone()
    }

    fn collect_isr(&mut self) {
        let mut isr_def_ids: HashSet<DefId> = HashSet::new();
        self.parsed_tags.iter().for_each(|tag_item| {
            if let LockTagItem::IsrEntry(did, _) = tag_item {
                isr_def_ids.insert(*did);
            }
        });

        self.program_isr_info.isr_entries = isr_def_ids.clone();

        let mut isr_funcs: HashSet<DefId> = HashSet::new();
        for isr_entry_id in isr_def_ids.iter() {
            isr_funcs.insert(*isr_entry_id);

            for callee in self.callgraph.get_callees_recursive(*isr_entry_id) {
                isr_funcs.insert(callee);
            }
        }

        for isr_func in isr_funcs.iter() {
            rtool_debug!(
                "Function {} may be a ISR function",
                self.tcx.def_path_str(*isr_func)
            );
        }

        self.program_isr_info.isr_funcs = isr_funcs;
    }

    fn collect_interrupt_apis(&mut self) {
        self.parsed_tags.iter().for_each(|tag_item| match tag_item {
            LockTagItem::IntrApi(did, is_enable, _is_nested, _) => {
                if *is_enable {
                    self.enable_interrupt_apis.push(*did);
                } else {
                    self.disable_interrupt_apis.push(*did);
                }
            }
            LockTagItem::LockOp(did, _lock_arg, guard_irq_disabled, _) => {
                self.lock_ops.insert(*did, *guard_irq_disabled);
            }
            _ => {}
        });
    }

    fn analyze_interrupt_set(&mut self) {
        let mut analyzed_functions: HashMap<DefId, FuncIrqInfo> = HashMap::new();
        let mut recursion_stack: HashSet<DefId> = HashSet::new();

        for local_def_id in self.tcx.hir_body_owners() {
            if let Some(_other) = self.tcx.hir_body_const_context(local_def_id) {
                continue;
            }

            let def_id = local_def_id.to_def_id();
            if self.tcx.is_mir_available(def_id) {
                self.analyze_function_interrupt_set(
                    def_id,
                    &mut analyzed_functions,
                    &mut recursion_stack,
                );
            }
        }

        for (def_id, func_info) in analyzed_functions {
            self.program_isr_info
                .func_irq_infos
                .insert(def_id, func_info);
        }
    }

    fn analyze_function_interrupt_set(
        &self,
        func_def_id: DefId,
        analyzed_functions: &mut HashMap<DefId, FuncIrqInfo>,
        recursion_stack: &mut HashSet<DefId>,
    ) {
        if analyzed_functions.get(&func_def_id).is_some() || recursion_stack.contains(&func_def_id)
        {
            return;
        }

        if !self.tcx.is_mir_available(func_def_id) {
            return;
        }

        recursion_stack.insert(func_def_id);

        for callee in self.callgraph.get_callees(func_def_id) {
            self.analyze_function_interrupt_set(callee, analyzed_functions, recursion_stack);
        }

        let body: &Body = self.tcx.optimized_mir(func_def_id);
        let func_lockmap = self.program_lock_info.lockmap.get(&func_def_id);
        let empty_lockmap = LocalLockMap::new();
        let lockmap = func_lockmap.unwrap_or(&empty_lockmap);
        let mut result_cursor = FuncIsrAnalyzer::new(
            self.tcx,
            self.enable_interrupt_apis.clone(),
            self.disable_interrupt_apis.clone(),
            self.lock_ops.clone(),
            lockmap,
            analyzed_functions,
        )
        .iterate_to_fixpoint(self.tcx, body, None)
        .into_results_cursor(body);

        let mut pre_bb_irq_states = HashMap::new();
        let mut exit_irq_state = IrqState::new();
        for (bb, _) in body.basic_blocks.iter_enumerated() {
            result_cursor.seek_to_block_start(bb);
            pre_bb_irq_states.insert(bb, result_cursor.get().effective_irq_state());

            result_cursor.seek_to_block_end(bb);
            let current_state = result_cursor.get().effective_irq_state();

            let loc = body.terminator_loc(bb);
            let terminator = body.stmt_at(loc).right().unwrap();
            if let TerminatorKind::Return = terminator.kind {
                exit_irq_state = exit_irq_state.union(&current_state);
            }
        }

        analyzed_functions.insert(
            func_def_id,
            FuncIrqInfo {
                def_id: func_def_id,
                exit_irq_state,
                pre_bb_irq_states,
                interrupt_enable_sites: Vec::new(),
            },
        );

        recursion_stack.remove(&func_def_id);
    }

    pub fn print_result(&self) {
        rtool_info!("==== ISR Analysis Results ====");

        for isr_func in self.program_isr_info.isr_funcs.iter() {
            rtool_info!("May be ISR func: {} ", self.tcx.def_path_str(*isr_func));
        }

        let mut count = 0;
        for (def_id, func_info) in self.program_isr_info.func_irq_infos.iter() {
            if func_info.exit_irq_state == IrqState::Bottom {
                continue;
            }
            rtool_info!(
                "Func: {},\t IRQ {}",
                self.tcx.def_path_str(*def_id),
                func_info
            );
            count += 1;
        }
        rtool_info!(
            "==== ISR Analysis Results End ({} ISR entries, {} non-trivial interrupt set functions) ====",
            self.program_isr_info.isr_entries.len(),
            count
        );
    }
}
