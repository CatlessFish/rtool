use rustc_ast::token::{Token, TokenKind};
use rustc_ast::tokenstream::{TokenStream, TokenTree};
use rustc_hir::definitions::DefPathData;
use rustc_hir::{AttrArgs, Attribute, BodyOwnerKind, def::DefKind, def_id::DefId};
use rustc_middle::ty::TyCtxt;
use rustc_span::Span;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::analysis::utils::def_path::def_path_def_ids;

/// Path segments suitable for [`def_path_def_ids`]: `crate::module::Type::method` without turbofish
/// (`::<...>`), so splitting on `::` is unambiguous.
fn def_path_segments_for_resolve(tcx: TyCtxt<'_>, def_id: DefId) -> Vec<String> {
    let mut segs = vec![tcx.crate_name(def_id.krate).as_str().to_string()];
    for el in tcx.def_path(def_id).data.iter() {
        match el.data {
            DefPathData::TypeNs(name) | DefPathData::ValueNs(name) | DefPathData::MacroNs(name) => {
                segs.push(name.as_str().to_string());
            }
            DefPathData::LifetimeNs(name) | DefPathData::OpaqueLifetime(name) => {
                segs.push(name.as_str().to_string());
            }
            DefPathData::CrateRoot
            | DefPathData::Ctor
            | DefPathData::Impl
            | DefPathData::ForeignMod
            | DefPathData::Use
            | DefPathData::GlobalAsm
            | DefPathData::Closure
            | DefPathData::AnonConst
            | DefPathData::LateAnonConst
            | DefPathData::DesugaredAnonymousLifetime
            | DefPathData::OpaqueTy
            | DefPathData::AnonAssocTy(_)
            | DefPathData::SyntheticCoroutineBody
            | DefPathData::NestedStatic => {}
        }
    }
    segs
}

/// Split `a::b::c` on `::` but not turbofish `::<`, so `spin::mutex::Mutex::<T, R>::lock` becomes
/// four logical segments (third still carries `::<T, R>` for [`normalize_path_segment`]).
fn split_def_path_skipping_turbofish(path: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let b = path.as_bytes();
    while i < b.len() {
        if i + 1 < b.len() && b[i] == b':' && b[i + 1] == b':' {
            if i + 2 < b.len() && b[i + 2] == b'<' {
                i += 2;
                continue;
            }
            out.push(path[start..i].trim());
            i += 2;
            start = i;
            continue;
        }
        i += 1;
    }
    out.push(path[start..].trim());
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

fn normalize_path_segment(seg: &str) -> String {
    let seg = seg.trim();
    if let Some(pos) = seg.find("::<") {
        return seg[..pos].to_string();
    }
    if let Some(pos) = seg.find('<') {
        return seg[..pos].trim_end_matches(':').to_string();
    }
    seg.to_string()
}

pub struct TagParser<'tcx> {
    tcx: TyCtxt<'tcx>,
}

#[derive(Debug, Clone)]
pub enum LockTagItem {
    LockType(DefId, String, SerializableSpan),
    LockGuardType(DefId, String, SerializableSpan),
    LockOp(
        DefId,
        usize, // LockArg
        bool,  // GuardIrqDisabled
        SerializableSpan,
    ),
    IntrApi(
        DefId,
        bool, // true = Enable, false = Disable
        bool, // Nested
        SerializableSpan,
    ),
    IsrEntry(DefId, SerializableSpan),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct SerializableDefId {
    pub crate_name: String,
    pub def_path: String,
}

impl SerializableDefId {
    pub fn from_def_id(tcx: TyCtxt<'_>, def_id: DefId) -> Self {
        let crate_num = def_id.krate;
        let crate_name = tcx.crate_name(crate_num).as_str().to_string();
        let segs = def_path_segments_for_resolve(tcx, def_id);
        SerializableDefId {
            def_path: segs.join("::"),
            crate_name,
        }
    }

    pub fn resolve(&self, tcx: TyCtxt<'_>) -> Option<DefId> {
        let raw: Vec<&str> = split_def_path_skipping_turbofish(&self.def_path);
        let normalized: Vec<String> = raw.iter().map(|s| normalize_path_segment(s)).collect();
        let path_refs: Vec<&str> = normalized.iter().map(|s| s.as_str()).collect();
        def_path_def_ids(&tcx, &path_refs).last()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SerializableSpan {
    pub lo: u32,
    pub hi: u32,
}

impl From<Span> for SerializableSpan {
    fn from(span: Span) -> Self {
        SerializableSpan {
            lo: span.lo().0,
            hi: span.hi().0,
        }
    }
}

impl From<SerializableSpan> for Span {
    fn from(span: SerializableSpan) -> Self {
        use rustc_span::BytePos;
        Span::with_root_ctxt(BytePos(span.lo), BytePos(span.hi))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum SerializableLockTagItem {
    LockType(SerializableDefId, String, SerializableSpan),
    LockGuardType(SerializableDefId, String, SerializableSpan),
    LockOp(SerializableDefId, usize, bool, SerializableSpan),
    IntrApi(SerializableDefId, bool, bool, SerializableSpan),
    IsrEntry(SerializableDefId, SerializableSpan),
}

impl SerializableLockTagItem {
    fn from_runtime(tcx: TyCtxt<'_>, item: &LockTagItem) -> Self {
        match item {
            LockTagItem::LockType(def_id, name, span) => Self::LockType(
                SerializableDefId::from_def_id(tcx, *def_id),
                name.clone(),
                span.clone(),
            ),
            LockTagItem::LockGuardType(def_id, name, span) => Self::LockGuardType(
                SerializableDefId::from_def_id(tcx, *def_id),
                name.clone(),
                span.clone(),
            ),
            LockTagItem::LockOp(def_id, lock_arg, guard_irq_disabled, span) => Self::LockOp(
                SerializableDefId::from_def_id(tcx, *def_id),
                *lock_arg,
                *guard_irq_disabled,
                span.clone(),
            ),
            LockTagItem::IntrApi(def_id, is_enable, is_nested, span) => Self::IntrApi(
                SerializableDefId::from_def_id(tcx, *def_id),
                *is_enable,
                *is_nested,
                span.clone(),
            ),
            LockTagItem::IsrEntry(def_id, span) => {
                Self::IsrEntry(SerializableDefId::from_def_id(tcx, *def_id), span.clone())
            }
        }
    }

    fn resolve(&self, tcx: TyCtxt<'_>) -> Option<LockTagItem> {
        match self {
            Self::LockType(def_id, name, span) => def_id
                .resolve(tcx)
                .map(|did| LockTagItem::LockType(did, name.clone(), span.clone())),
            Self::LockGuardType(def_id, name, span) => def_id
                .resolve(tcx)
                .map(|did| LockTagItem::LockGuardType(did, name.clone(), span.clone())),
            Self::LockOp(def_id, lock_arg, guard_irq_disabled, span) => def_id
                .resolve(tcx)
                .map(|did| LockTagItem::LockOp(did, *lock_arg, *guard_irq_disabled, span.clone())),
            Self::IntrApi(def_id, is_enable, is_nested, span) => def_id
                .resolve(tcx)
                .map(|did| LockTagItem::IntrApi(did, *is_enable, *is_nested, span.clone())),
            Self::IsrEntry(def_id, span) => def_id
                .resolve(tcx)
                .map(|did| LockTagItem::IsrEntry(did, span.clone())),
        }
    }

    fn def_id(&self) -> &SerializableDefId {
        match self {
            Self::LockType(def_id, ..)
            | Self::LockGuardType(def_id, ..)
            | Self::LockOp(def_id, ..)
            | Self::IntrApi(def_id, ..)
            | Self::IsrEntry(def_id, ..) => def_id,
        }
    }
}

// Helper function: parse format "Name = \"SomeName\""
fn parse_name_value(tokens: &TokenStream) -> Option<String> {
    let mut iter = tokens.iter();

    // Look for pattern Name = "value"
    while let Some(tree) = iter.next() {
        if let TokenTree::Token(
            Token {
                kind: TokenKind::Ident(sym, _),
                ..
            },
            _,
        ) = tree
        {
            if sym.as_str() == "Name" {
                // Expect '='
                if let Some(TokenTree::Token(
                    Token {
                        kind: TokenKind::Eq,
                        ..
                    },
                    _,
                )) = iter.next()
                {
                    // Expect string literal
                    if let Some(TokenTree::Token(
                        Token {
                            kind: TokenKind::Literal(lit),
                            ..
                        },
                        _,
                    )) = iter.next()
                    {
                        let s = lit.symbol.as_str();
                        // Remove quotes
                        return Some(s.trim_matches('"').to_string());
                    }
                }
            }
        }
    }
    None
}

// Helper function: parse format "Type = Enable/Disable, Nested = true/false"
fn parse_intr_api(tokens: &TokenStream) -> Option<(bool, bool)> {
    let mut iter = tokens.iter();
    let mut typ_value: Option<bool> = None;
    let mut nested_value: Option<bool> = None;

    while let Some(tree) = iter.next() {
        if let TokenTree::Token(
            Token {
                kind: TokenKind::Ident(sym, _),
                ..
            },
            _,
        ) = tree
        {
            let key = sym.as_str();

            if key == "Type" {
                // Expect '='
                if let Some(TokenTree::Token(
                    Token {
                        kind: TokenKind::Eq,
                        ..
                    },
                    _,
                )) = iter.next()
                {
                    // Expect Enable or Disable
                    if let Some(TokenTree::Token(
                        Token {
                            kind: TokenKind::Ident(val_sym, _),
                            ..
                        },
                        _,
                    )) = iter.next()
                    {
                        match val_sym.as_str() {
                            "Enable" => typ_value = Some(true),
                            "Disable" => typ_value = Some(false),
                            _ => return None,
                        }
                    }
                }
            } else if key == "Nested" {
                // Expect '='
                if let Some(TokenTree::Token(
                    Token {
                        kind: TokenKind::Eq,
                        ..
                    },
                    _,
                )) = iter.next()
                {
                    // Expect true or false
                    if let Some(TokenTree::Token(
                        Token {
                            kind: TokenKind::Ident(val_sym, _),
                            ..
                        },
                        _,
                    )) = iter.next()
                    {
                        match val_sym.as_str() {
                            "true" => nested_value = Some(true),
                            "false" => nested_value = Some(false),
                            _ => return None,
                        }
                    }
                }
            }
        }
    }

    // Both values must exist
    match (typ_value, nested_value) {
        (Some(t), Some(n)) => Some((t, n)),
        _ => None,
    }
}

fn parse_lock_op(tokens: &TokenStream) -> Option<(usize, bool)> {
    let mut iter = tokens.iter();
    let mut lock_arg = None;
    let mut guard_irq_disabled = None;

    while let Some(tree) = iter.next() {
        let TokenTree::Token(
            Token {
                kind: TokenKind::Ident(sym, _),
                ..
            },
            _,
        ) = tree
        else {
            continue;
        };

        let key = sym.as_str();
        let Some(TokenTree::Token(
            Token {
                kind: TokenKind::Eq,
                ..
            },
            _,
        )) = iter.next()
        else {
            continue;
        };

        match key {
            "LockArg" => {
                let Some(TokenTree::Token(
                    Token {
                        kind: TokenKind::Literal(lit),
                        ..
                    },
                    _,
                )) = iter.next()
                else {
                    return None;
                };
                lock_arg = lit.symbol.as_str().parse::<usize>().ok();
                if lock_arg.is_none() {
                    return None;
                }
            }
            "GuardIrqDisabled" => {
                let Some(TokenTree::Token(
                    Token {
                        kind: TokenKind::Ident(val_sym, _),
                        ..
                    },
                    _,
                )) = iter.next()
                else {
                    return None;
                };
                guard_irq_disabled = match val_sym.as_str() {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => return None,
                };
            }
            _ => {}
        }
    }

    match (lock_arg, guard_irq_disabled) {
        (Some(lock_arg), Some(guard_irq_disabled)) => Some((lock_arg, guard_irq_disabled)),
        _ => None,
    }
}

pub fn extract_locktag_item(did: DefId, attr: &Attribute) -> Option<LockTagItem> {
    match attr {
        Attribute::Parsed(_) => None,
        Attribute::Unparsed(box attr) => {
            let path = attr.path.segments.clone().into_vec();
            // expect at least ["rapx", "{some_attr}"]
            if path.len() < 2 {
                return None;
            };
            if path[0].as_str() != "rapx" {
                return None;
            }

            // expect delimited key-value pairs like "(Type = Enable)"
            let tokens = match &attr.args {
                AttrArgs::Delimited(delim) => delim.tokens.clone(),
                AttrArgs::Empty => {
                    if path[1].as_str() == "IsrEntry" {
                        return Some(LockTagItem::IsrEntry(did, attr.span.into()));
                    } else {
                        return None;
                    }
                }
                _ => return None,
            };
            match path[1].as_str() {
                "LockType" => {
                    let name = parse_name_value(&tokens);
                    match name {
                        Some(n) => Some(LockTagItem::LockType(did, n, attr.span.into())),
                        None => {
                            crate::rtool_warn!("Failed to parse LockType attribute for {:?}", did);
                            None
                        }
                    }
                }
                "LockGuardType" => {
                    let name = parse_name_value(&tokens);
                    match name {
                        Some(n) => Some(LockTagItem::LockGuardType(did, n, attr.span.into())),
                        None => {
                            crate::rtool_warn!(
                                "Failed to parse LockGuardType attribute for {:?}",
                                did
                            );
                            None
                        }
                    }
                }
                "LockOp" => match parse_lock_op(&tokens) {
                    Some((lock_arg, guard_irq_disabled)) => Some(LockTagItem::LockOp(
                        did,
                        lock_arg,
                        guard_irq_disabled,
                        attr.span.into(),
                    )),
                    None => {
                        crate::rtool_warn!("Failed to parse LockOp attribute for {:?}", did);
                        None
                    }
                },
                "IntrApi" => match parse_intr_api(&tokens) {
                    Some((typ, nested)) => {
                        Some(LockTagItem::IntrApi(did, typ, nested, attr.span.into()))
                    }
                    None => {
                        crate::rtool_warn!("Failed to parse IntrApi attribute for {:?}", did);
                        None
                    }
                },
                _ => {
                    crate::rtool_warn!("Unsupported Lock Tag: {}", path[1].as_str());
                    None
                }
            }
        }
    }
}

impl<'tcx> TagParser<'tcx> {
    pub fn new(tcx: TyCtxt<'tcx>) -> Self {
        Self { tcx }
    }

    fn collect_tags_for_def_id<F>(&self, did: DefId, filter: F) -> Vec<LockTagItem>
    where
        F: Fn(&LockTagItem) -> bool,
    {
        let attrs = if did.is_local() {
            self.tcx
                .hir_attrs(self.tcx.local_def_id_to_hir_id(did.expect_local()))
        } else {
            self.tcx.get_all_attrs(did)
        };

        attrs
            .iter()
            .filter_map(|attr| extract_locktag_item(did, attr))
            .filter(filter)
            .collect()
    }

    pub fn load_analyze_save(
        &self,
        load_path: Option<&str>,
        save_path: Option<&str>,
    ) -> Vec<LockTagItem> {
        let mut persisted_tags = if let Some(load_path) = load_path {
            match std::fs::read_to_string(load_path) {
                Ok(content) => match serde_json::from_str::<Vec<SerializableLockTagItem>>(&content)
                {
                    Ok(loaded) => {
                        crate::rtool_info!(
                            "Loaded {} serialized tags from {}",
                            loaded.len(),
                            load_path
                        );
                        loaded
                    }
                    Err(e) => {
                        crate::rtool_warn!("Failed to parse tags from {}: {}", load_path, e);
                        vec![]
                    }
                },
                Err(e) => {
                    crate::rtool_warn!("Failed to read tag file {}: {}", load_path, e);
                    vec![]
                }
            }
        } else {
            vec![]
        };

        let mut unresolved_cached_tags = 0;
        let mut reported_unresolved_paths: HashSet<String> = HashSet::new();
        let mut tags: Vec<LockTagItem> = persisted_tags
            .iter()
            .filter_map(|tag| match tag.resolve(self.tcx) {
                Some(tag) => Some(tag),
                None => {
                    unresolved_cached_tags += 1;
                    let def_id = tag.def_id();
                    if reported_unresolved_paths.insert(def_id.def_path.clone()) {
                        crate::rtool_warn!(
                            "Failed to resolve cached item {} from crate {} in current session",
                            def_id.def_path,
                            def_id.crate_name,
                        );
                    }
                    crate::rtool_warn!(
                        "Skipping cached tag for item {} from crate {} because it is unavailable in the current session",
                        def_id.def_path,
                        def_id.crate_name,
                    );
                    None
                }
            })
            .collect();
        if unresolved_cached_tags > 0 {
            crate::rtool_warn!(
                "Skipped {} cached tags that could not be resolved in this compilation session",
                unresolved_cached_tags
            );
        }

        let analyzed_tags = self.analyze_current_crate();
        persisted_tags.extend(
            analyzed_tags
                .iter()
                .map(|tag| SerializableLockTagItem::from_runtime(self.tcx, tag)),
        );
        tags.extend(analyzed_tags);

        if let Some(save_path) = save_path {
            match serde_json::to_string_pretty(&persisted_tags) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(save_path, json) {
                        crate::rtool_warn!("Failed to save tags to {}: {}", save_path, e);
                    } else {
                        crate::rtool_info!("Saved tags to {}", save_path);
                    }
                }
                Err(e) => {
                    crate::rtool_warn!("Failed to serialize tags to JSON: {}", e);
                }
            }
        }
        tags
    }

    fn analyze_current_crate(&self) -> Vec<LockTagItem> {
        let mut result = vec![];

        for id in self.tcx.hir_free_items() {
            let item = self.tcx.hir_item(id);
            let did = item.owner_id.def_id.to_def_id();
            result.extend(self.collect_tags_for_def_id(did, |tag| {
                matches!(
                    tag,
                    LockTagItem::LockType(..) | LockTagItem::LockGuardType(..)
                )
            }));
        }

        for local_def_id in self.tcx.hir_body_owners() {
            if !matches!(
                self.tcx.hir_body_owner_kind(local_def_id),
                BodyOwnerKind::Fn
            ) {
                continue;
            }

            let did = local_def_id.to_def_id();
            if matches!(self.tcx.def_kind(did), DefKind::Closure) {
                continue;
            }

            result.extend(self.collect_tags_for_def_id(did, |tag| {
                matches!(
                    tag,
                    LockTagItem::LockOp(..) | LockTagItem::IntrApi(..) | LockTagItem::IsrEntry(..)
                )
            }));
        }

        let mut lock_type_count = 0;
        let mut lock_guard_type_count = 0;
        let mut lock_op_count = 0;
        let mut intr_api_count = 0;
        let mut isr_entry_count = 0;
        for item in &result {
            match item {
                LockTagItem::LockType(..) => lock_type_count += 1,
                LockTagItem::LockGuardType(..) => lock_guard_type_count += 1,
                LockTagItem::LockOp(..) => lock_op_count += 1,
                LockTagItem::IntrApi(..) => intr_api_count += 1,
                LockTagItem::IsrEntry(..) => isr_entry_count += 1,
            }
        }
        crate::rtool_info!(
            "Tags found: LockType = {}, LockGuardType = {}, LockOp = {}, IntrApi = {}, IsrEntry = {}",
            lock_type_count,
            lock_guard_type_count,
            lock_op_count,
            intr_api_count,
            isr_entry_count
        );
        result
    }
}
