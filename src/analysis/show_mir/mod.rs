use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use crate::{rtool_error, rtool_info};
use colorful::{Color, Colorful};
use rustc_hir::def_id::DefId;
use rustc_middle::mir::{
    BasicBlockData, BasicBlocks, Body, LocalDecl, LocalDecls, Operand, Rvalue, Statement,
    StatementKind, Terminator, TerminatorKind,
};
use rustc_middle::ty::{self, TyCtxt, TyKind};

const NEXT_LINE: &str = "\n";
const PADDING: &str = "    ";
const EXPLAIN: &str = " @ ";

// This trait is a wrapper towards std::Display or std::Debug, and is to resolve orphan restrictions.
pub trait Display {
    fn display(&self) -> String;
}

impl<'tcx> Display for Terminator<'tcx> {
    fn display(&self) -> String {
        let mut s = String::new();
        s += &format!("{}{:?}{}", PADDING, self.kind, self.kind.display());
        s
    }
}

impl<'tcx> Display for TerminatorKind<'tcx> {
    fn display(&self) -> String {
        let mut s = String::new();
        s += EXPLAIN;
        match &self {
            TerminatorKind::Goto { .. } => s += "Goto",
            TerminatorKind::SwitchInt { .. } => s += "SwitchInt",
            TerminatorKind::Return => s += "Return",
            TerminatorKind::Unreachable => s += "Unreachable",
            TerminatorKind::Drop { .. } => s += "Drop",
            TerminatorKind::Assert { .. } => s += "Assert",
            TerminatorKind::Yield { .. } => s += "Yield",
            TerminatorKind::FalseEdge { .. } => s += "FalseEdge",
            TerminatorKind::FalseUnwind { .. } => s += "FalseUnwind",
            TerminatorKind::InlineAsm { .. } => s += "InlineAsm",
            TerminatorKind::UnwindResume => s += "UnwindResume",
            TerminatorKind::UnwindTerminate(..) => s += "UnwindTerminate",
            TerminatorKind::CoroutineDrop => s += "CoroutineDrop",
            TerminatorKind::Call { func, .. } => match func {
                Operand::Constant(constant) => match constant.ty().kind() {
                    ty::FnDef(id, ..) => {
                        s += &format!("Call: FnDid: {}", id.index.as_usize()).as_str()
                    }
                    _ => (),
                },
                _ => (),
            },
            TerminatorKind::TailCall { .. } => todo!(),
        };
        s
    }
}

impl<'tcx> Display for Statement<'tcx> {
    fn display(&self) -> String {
        let mut s = String::new();
        s += &format!("{}{:?}{}", PADDING, self.kind, self.kind.display());
        s
    }
}

impl<'tcx> Display for StatementKind<'tcx> {
    fn display(&self) -> String {
        let mut s = String::new();
        s += EXPLAIN;
        match &self {
            StatementKind::Assign(assign) => {
                s += &format!("{:?}={:?}{}", assign.0, assign.1, assign.1.display());
            }
            StatementKind::FakeRead(..) => s += "FakeRead",
            StatementKind::SetDiscriminant { .. } => s += "SetDiscriminant",
            StatementKind::StorageLive(..) => s += "StorageLive",
            StatementKind::StorageDead(..) => s += "StorageDead",
            StatementKind::Retag(..) => s += "Retag",
            StatementKind::AscribeUserType(..) => s += "AscribeUserType",
            StatementKind::Coverage(..) => s += "Coverage",
            StatementKind::Nop => s += "Nop",
            StatementKind::PlaceMention(..) => s += "PlaceMention",
            StatementKind::Intrinsic(..) => s += "Intrinsic",
            StatementKind::ConstEvalCounter => s += "ConstEvalCounter",
            _ => todo!(),
        }
        s
    }
}

impl<'tcx> Display for Rvalue<'tcx> {
    fn display(&self) -> String {
        let mut s = String::new();
        s += EXPLAIN;
        match self {
            Rvalue::Use(..) => s += "Use",
            Rvalue::Repeat(..) => s += "Repeat",
            Rvalue::Ref(..) => s += "Ref",
            Rvalue::ThreadLocalRef(..) => s += "ThreadLocalRef",
            Rvalue::Cast(..) => s += "Cast",
            Rvalue::BinaryOp(..) => s += "BinaryOp",
            Rvalue::NullaryOp(..) => s += "NullaryOp",
            Rvalue::UnaryOp(..) => s += "UnaryOp",
            Rvalue::Discriminant(..) => s += "Discriminant",
            Rvalue::Aggregate(..) => s += "Aggregate",
            Rvalue::ShallowInitBox(..) => s += "ShallowInitBox",
            Rvalue::CopyForDeref(..) => s += "CopyForDeref",
            Rvalue::RawPtr(_, _) => s += "RawPtr",
            _ => todo!(),
        }
        s
    }
}

impl<'tcx> Display for BasicBlocks<'tcx> {
    fn display(&self) -> String {
        let mut s = String::new();
        for (index, bb) in self.iter().enumerate() {
            s += &format!(
                "bb {} {{{}{}}}{}",
                index,
                NEXT_LINE,
                bb.display(),
                NEXT_LINE
            );
        }
        s
    }
}

impl<'tcx> Display for BasicBlockData<'tcx> {
    fn display(&self) -> String {
        let mut s = String::new();
        s += &format!("CleanUp: {}{}", self.is_cleanup, NEXT_LINE);
        for stmt in self.statements.iter() {
            s += &format!("{}{}", stmt.display(), NEXT_LINE);
        }
        s += &format!(
            "{}{}",
            self.terminator.clone().unwrap().display(),
            NEXT_LINE
        );
        s
    }
}

impl<'tcx> Display for LocalDecls<'tcx> {
    fn display(&self) -> String {
        let mut s = String::new();
        for (index, ld) in self.iter().enumerate() {
            s += &format!("_{}: {} {}", index, ld.display(), NEXT_LINE);
        }
        s
    }
}

impl<'tcx> Display for LocalDecl<'tcx> {
    fn display(&self) -> String {
        let mut s = String::new();
        s += &format!("{}{}", EXPLAIN, self.ty.kind().display());
        s
    }
}

impl<'tcx> Display for Body<'tcx> {
    fn display(&self) -> String {
        let mut s = String::new();
        s += &self.local_decls.display();
        s += &self.basic_blocks.display();
        s
    }
}

impl<'tcx> Display for TyKind<'tcx> {
    fn display(&self) -> String {
        let mut s = String::new();
        s += &format!("{:?}", self);
        s
    }
}

impl Display for DefId {
    fn display(&self) -> String {
        format!("{:?}", self)
    }
}

// #[inline(always)]
pub fn display_mir_colored(did: DefId, body: &Body) {
    rtool_info!("{}", did.display().color(Color::LightRed));
    rtool_info!("{}", body.local_decls.display().color(Color::Green));
    rtool_info!(
        "{}",
        body.basic_blocks.display().color(Color::LightGoldenrod2a)
    );
}

pub fn display_mir_plain(name: &String, body: &Body, writer: &mut Box<dyn Write>) {
    match display_mir_plain_inner(name, body, writer) {
        Ok(_) => {}
        Err(e) => {
            rtool_error!("{}", e.to_string())
        }
    }
}

fn display_mir_plain_inner(
    name: &String,
    body: &Body,
    writer: &mut Box<dyn Write>,
) -> Result<(), io::Error> {
    writer.write_fmt(format_args!("fn {}\n", name))?;
    writer.write_fmt(format_args!("{}\n", body.local_decls.display()))?;
    writer.write_fmt(format_args!("{}\n", body.basic_blocks.display()))?;
    writer.flush()
}

pub fn display_bb_source_info<'tcx>(tcx: TyCtxt<'tcx>, body: &Body, writer: &mut Box<dyn Write>) {
    match display_bb_source_info_inner(tcx, body, writer) {
        Ok(_) => {}
        Err(e) => {
            rtool_error!("{}", e.to_string());
        }
    }
}

fn display_bb_source_info_inner<'tcx>(
    tcx: TyCtxt<'tcx>,
    body: &Body,
    writer: &mut Box<dyn Write>,
) -> Result<(), io::Error> {
    for (idx, bb) in body.basic_blocks.iter_enumerated() {
        if bb.statements.len() == 0 {
            continue;
        }
        let stmt = &bb.statements[0];
        let span = stmt.source_info.span;
        let smap = tcx.sess.source_map();
        writer.write_fmt(format_args!(
            "{:?} at {}\n",
            idx,
            smap.span_to_diagnostic_string(span)
        ))?
    }
    Ok(())
}

pub struct ShowAllMir<'tcx> {
    pub tcx: TyCtxt<'tcx>,
}

impl<'tcx> ShowAllMir<'tcx> {
    pub fn new(tcx: TyCtxt<'tcx>) -> Self {
        Self { tcx }
    }

    pub fn start(&mut self) {
        rtool_info!("Show all MIR");
        let mir_keys = self.tcx.mir_keys(());
        for each_mir in mir_keys {
            let def_id = each_mir.to_def_id();
            let body = self.tcx.instance_mir(ty::InstanceKind::Item(def_id));
            display_mir_colored(def_id, body);
        }
    }
}

pub struct FindAndShowMir<'tcx, 'a> {
    pub tcx: TyCtxt<'tcx>,
    pub exact_fn_names: &'a Vec<String>,
    pub fuzzy_fn_names: &'a Vec<String>,
    pub output_file: Option<String>,
}

impl<'tcx, 'a> FindAndShowMir<'tcx, 'a> {
    pub fn new(
        tcx: TyCtxt<'tcx>,
        exact_fn_names: &'a Vec<String>,
        fuzzy_fn_names: &'a Vec<String>,
        output_file: Option<String>,
    ) -> Self {
        Self {
            tcx,
            exact_fn_names,
            fuzzy_fn_names,
            output_file,
        }
    }

    pub fn start(&mut self) {
        let mut out_writer = match self.output_file {
            Some(ref path) => {
                let os_path = Path::new(path);
                Box::new(File::create(&os_path).unwrap()) as Box<dyn Write>
            }
            None => Box::new(io::stdout()) as Box<dyn Write>,
        };
        let mir_keys = self.tcx.mir_keys(());
        rtool_info!("Exact match target: {:?}", { self.exact_fn_names });
        rtool_info!("Fuzzy match target: {:?}", { self.fuzzy_fn_names });
        for each_mir in mir_keys {
            let def_id = each_mir.to_def_id();
            let fn_name = self.tcx.def_path_str(def_id);
            let def_id_str = format!("{:?}", def_id);
            // rtool_info!("Checking {}", fn_name);
            if self
                .exact_fn_names
                .iter()
                .any(|target| *target == fn_name || def_id_str.contains(target))
            {
                let body = self.tcx.instance_mir(ty::InstanceKind::Item(def_id));
                rtool_info!("{}", def_id.display().color(Color::LightBlue));
                display_bb_source_info(self.tcx, body, &mut out_writer);
                display_mir_plain(&fn_name, body, &mut out_writer);
            }
            if self.fuzzy_fn_names.iter().any(|fuzzy_name| {
                let real_fn_name = fn_name.split("::").last().unwrap_or("");
                real_fn_name.contains(fuzzy_name)
            }) {
                let body = self.tcx.instance_mir(ty::InstanceKind::Item(def_id));
                rtool_info!("{}", def_id.display().color(Color::LightBlue));
                display_bb_source_info(self.tcx, body, &mut out_writer);
                display_mir_plain(&fn_name, body, &mut out_writer);
            }
        }
    }
}
