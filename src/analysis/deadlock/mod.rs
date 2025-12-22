pub mod deadlock_reporter;
pub mod isr_analyzer;
pub mod ldg_constructor;
pub mod lock_collector;
pub mod lockset_analyzer;
pub mod tag_parser;
pub mod types;

use crate::analysis::callgraph::default::{CallGraphAnalyzer, CallGraphInfo};
use crate::analysis::deadlock::deadlock_reporter::DeadlockReporter;
use crate::analysis::deadlock::isr_analyzer::IsrAnalyzer;
use crate::analysis::deadlock::ldg_constructor::LDGConstructor;
use crate::analysis::deadlock::lock_collector::LockCollector;
use crate::analysis::deadlock::lockset_analyzer::LockSetAnalyzer;
use crate::analysis::deadlock::tag_parser::{LockTagItem, TagParser};
use crate::analysis::deadlock::types::{LockDependencyGraph, interrupt::*, lock::*};
use crate::rtool_info;
use rustc_middle::ty::TyCtxt;

pub struct DeadlockDetector<'tcx> {
    pub tcx: TyCtxt<'tcx>,
    pub callgraph: CallGraphInfo<'tcx>,

    parsed_tags: Vec<LockTagItem>,
    program_lock_info: ProgramLockInfo,
    program_lock_set: ProgramLockSet,
    program_isr_info: ProgramIsrInfo,
    lock_dependency_graph: LockDependencyGraph,
}

impl<'tcx> DeadlockDetector<'tcx> {
    pub fn new(tcx: TyCtxt<'tcx>) -> Self {
        Self {
            tcx,
            callgraph: CallGraphInfo::new(),
            parsed_tags: vec![],
            program_lock_info: ProgramLockInfo::new(),
            program_lock_set: ProgramLockSet::new(),
            program_isr_info: ProgramIsrInfo::new(),
            lock_dependency_graph: LockDependencyGraph::new(),
        }
    }

    /// Start Interrupt-Aware Deadlock Detection
    /// Note: the detection is currently crate-local
    pub fn run(&mut self) {
        rtool_info!("Executing Deadlock Detection");

        // Steps:
        // Dependencies
        let mut callgraph_analyzer = CallGraphAnalyzer::new(self.tcx);
        callgraph_analyzer.start();
        self.callgraph = callgraph_analyzer.graph;

        // 0. Parse Tags
        let tag_parser = TagParser::new(self.tcx);
        self.parsed_tags = tag_parser.run();

        // 1. Identify ISRs and Analysis InterruptSet
        // let mut isr_analyzer = IsrAnalyzer::new(self.tcx, &self.callgraph, &self.parsed_tags);
        // self.program_isr_info = isr_analyzer.run();
        // isr_analyzer.print_result();

        // 2. Collect Locks and LockGuards
        let mut lock_collector = LockCollector::new(self.tcx, &self.parsed_tags);
        self.program_lock_info = lock_collector.collect();
        lock_collector.print_result();

        // // 3. Analysis LockSet
        // let mut lockset_analyzer = LockSetAnalyzer::new(self.tcx, &self.program_lock_info.lockmap);
        // self.program_lock_set = lockset_analyzer.run();
        // // lockset_analyzer.print_result();

        // // 4. Construct Lock Dependency Graph
        // let mut ldg_constructor =
        //     LDGConstructor::new(self.tcx, &self.program_lock_set, &self.program_isr_info);
        // ldg_constructor.run();
        // ldg_constructor.print_result();
        // self.lock_dependency_graph = ldg_constructor.into_graph();

        // // 5. Detect cycles on LDG
        // let mut lock_reporter = DeadlockReporter::new(self.tcx, &self.lock_dependency_graph);
        // lock_reporter.run();
    }
}

// TODO:
// 1. test? correctness?
