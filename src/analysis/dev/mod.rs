use rustc_ast::token::{Token, TokenKind};
use rustc_ast::tokenstream::{TokenStream, TokenTree};
use rustc_hir::{AttrArgs, Attribute, def_id::DefId};
#[allow(unused)]
use rustc_middle::mir::{Body, Location, Statement, Terminator, TerminatorEdges, TerminatorKind};
use rustc_middle::ty::TyCtxt;
use rustc_span::Span;

use crate::{rtool_info, rtool_warn};

pub struct LockDevTool<'tcx> {
    tcx: TyCtxt<'tcx>,
}

#[derive(Debug)]
pub enum LockTagItem {
    LockType(
        DefId,
        String, // Name
        Span,
    ),
    LockGuardType(
        DefId,
        String, // Name
        Span,
    ),
    IntrApi(
        DefId,
        bool, // true = Enable, false = Disable
        bool, // Nested
    ),
}

// 辅助函数：解析 "Name = \"SomeName\"" 格式
fn parse_name_value(tokens: &TokenStream) -> Option<String> {
    let mut iter = tokens.iter();

    // 查找 Name = "value" 模式
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
                // 期待 '='
                if let Some(TokenTree::Token(
                    Token {
                        kind: TokenKind::Eq,
                        ..
                    },
                    _,
                )) = iter.next()
                {
                    // 期待字符串字面量
                    if let Some(TokenTree::Token(
                        Token {
                            kind: TokenKind::Literal(lit),
                            ..
                        },
                        _,
                    )) = iter.next()
                    {
                        let s = lit.symbol.as_str();
                        // 去除引号
                        return Some(s.trim_matches('"').to_string());
                    }
                }
            }
        }
    }
    None
}

// 辅助函数：解析 "Type = Enable/Disable, Nested = true/false" 格式
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
                // 期待 '='
                if let Some(TokenTree::Token(
                    Token {
                        kind: TokenKind::Eq,
                        ..
                    },
                    _,
                )) = iter.next()
                {
                    // 期待 Enable 或 Disable
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
                // 期待 '='
                if let Some(TokenTree::Token(
                    Token {
                        kind: TokenKind::Eq,
                        ..
                    },
                    _,
                )) = iter.next()
                {
                    // 期待 true 或 false
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

    // 两个值都必须存在
    match (typ_value, nested_value) {
        (Some(t), Some(n)) => Some((t, n)),
        _ => None,
    }
}

pub fn extract_locktag_item(did: DefId, attr: &Attribute) -> Option<LockTagItem> {
    match attr {
        Attribute::Parsed(_) => None,
        Attribute::Unparsed(box attr) => {
            let path = attr.path.segments.clone().into_vec();
            // expect ["rapx", "{some_attr}"] at least
            if path.len() < 2 {
                return None;
            };
            if path[0].as_str() != "rapx" {
                return None;
            }

            // expect delimited key-value pairs like "(Type = Enable)"
            let tokens = match &attr.args {
                AttrArgs::Delimited(delim) => delim.tokens.clone(),
                _ => return None,
            };
            match path[1].as_str() {
                "LockType" => {
                    // 解析 Name = "SpinLock" 格式
                    let name = parse_name_value(&tokens);
                    match name {
                        Some(n) => Some(LockTagItem::LockType(did, n, attr.span)),
                        None => {
                            rtool_warn!("Failed to parse LockType attribute for {:?}", did);
                            None
                        }
                    }
                }
                "LockGuardType" => {
                    // 解析 Name = "SpinLockGuard" 格式
                    let name = parse_name_value(&tokens);
                    match name {
                        Some(n) => Some(LockTagItem::LockGuardType(did, n, attr.span)),
                        None => {
                            rtool_warn!("Failed to parse LockGuardType attribute for {:?}", did);
                            None
                        }
                    }
                }
                "IntrApi" => {
                    // 解析 Type = Enable/Disable, Nested = true/false 格式
                    match parse_intr_api(&tokens) {
                        Some((typ, nested)) => Some(LockTagItem::IntrApi(did, typ, nested)),
                        None => {
                            rtool_warn!("Failed to parse IntrApi attribute for {:?}", did);
                            None
                        }
                    }
                }
                _ => {
                    rtool_warn!("Unsupported Lock Tag: {}", path[1].as_str());
                    None
                }
            }
        }
    }
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
                let tag_item = extract_locktag_item(did, attr);
                if let Some(item) = tag_item {
                    rtool_info!("{item:?}");
                }
            }
        }
    }
}
