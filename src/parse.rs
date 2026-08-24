//! Parse adapter: `syn` → normalized label tree + fragment extraction.
//! Confines the `syn` / `proc-macro2` dependencies to this module.
//!
//! Extraction is **EXTENDED** granularity (A1): free functions, methods
//! (inherent-impl, trait-impl and trait default-method bodies), `impl` block
//! bodies, closures, and free `{}` block expressions. Nested fragments overlap
//! their parents by construction — a closure inside a method is both lowered
//! into that method's tree *and* emitted as a fragment of its own. Removing the
//! resulting redundant findings is the containment-dedup policy's job
//! (`crate::dedup`, T9), not this module's.
//!
//! Lowering canonicalizes identifiers/literals and preserves structure; see
//! [`crate::tree`] for the normalization contract.

use proc_macro2::LineColumn;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{
    BinOp, Block, Expr, ExprBlock, ExprClosure, ImplItem, ImplItemFn, ItemFn, ItemImpl, Macro,
    MacroDelimiter, Pat, PointerMutability, RangeLimits, Signature, Stmt, TraitItemFn, UnOp,
};

use crate::error::{Error, Result};
use crate::model::{Analyzed, Fragment, FragmentKind};
use crate::tree::{BlockKind, Delimiter, Label, Mutability, NormTree, RangeKind};

/// Parse `source` (the contents of the `/`-normalized `path`) and extract every
/// fragment (A1's EXTENDED set), each paired with its normalized tree.
///
/// Results are ordered deterministically by `Fragment::canonical_key`. Nested
/// fragments are emitted alongside their enclosing ones and therefore overlap
/// them (see the module note). A parse failure is mapped to [`Error::Parse`]
/// with the `syn` error's line/column folded into the message (the variant
/// carries only a `String`), so location survives (N2).
pub(crate) fn extract(path: &str, source: &str) -> Result<Vec<Analyzed>> {
    let file = syn::parse_file(source).map_err(|err| parse_error(path, &err))?;
    let mut collector = Collector {
        path,
        out: Vec::new(),
    };
    collector.visit_file(&file);
    let mut out = collector.out;
    out.sort_by(|a, b| a.fragment.canonical_key().cmp(&b.fragment.canonical_key()));
    Ok(out)
}

/// Walks a parsed file and emits one [`Analyzed`] per extractable construct.
///
/// `syn`'s visitor supplies the traversal, so every construct is caught
/// wherever it appears (a closure in a trait default body, a free block inside
/// another closure, …) without this module re-enumerating the expression
/// grammar that [`lower_expr`] already covers.
struct Collector<'a> {
    /// The `/`-normalized path every emitted fragment carries.
    path: &'a str,
    /// Fragments found so far, in traversal order.
    out: Vec<Analyzed>,
}

impl Collector<'_> {
    /// Record one fragment from its normalized tree and 1-based line span.
    fn push(&mut self, kind: FragmentKind, tree: NormTree, (start_line, end_line): (usize, usize)) {
        let fragment = Fragment {
            path: self.path.to_string(),
            start_line,
            end_line,
            node_count: tree.node_count(),
            // N9: `line_count` is the physical span height `end - start + 1`
            // (the value `--min-lines` gates on in T5). Logical LOC was
            // considered for closer dry4go parity but the physical span is
            // unambiguous and stable.
            line_count: end_line - start_line + 1,
            kind,
        };
        self.out.push(Analyzed { fragment, tree });
    }

    /// Descend into an expression that is some construct's **body**.
    ///
    /// A braced body (`|x| { .. }`, `else { .. }`, `pat => { .. }`) belongs to
    /// its construct's shape, so it is walked for the fragments *inside* it
    /// without being emitted as a free block of its own.
    fn visit_body(&mut self, expr: &Expr) {
        match expr {
            Expr::Block(e) => self.visit_block(&e.block),
            other => self.visit_expr(other),
        }
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.push(
            FragmentKind::Function,
            lower_fn(&node.sig, &node.block),
            fn_span(&node.sig, &node.block),
        );
        visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        // The `impl` block as a whole: a repeated set of methods is itself a
        // clone signal, independent of each method matching individually.
        self.push(
            FragmentKind::ImplBlock,
            lower_impl(node),
            line_span(
                node.impl_token.span().start(),
                node.brace_token.span.close().end(),
            ),
        );
        visit::visit_item_impl(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        self.push(
            FragmentKind::Method,
            lower_fn(&node.sig, &node.block),
            fn_span(&node.sig, &node.block),
        );
        visit::visit_impl_item_fn(self, node);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast TraitItemFn) {
        // Only trait methods with a default body are fragments.
        if let Some(block) = &node.default {
            self.push(
                FragmentKind::Method,
                lower_fn(&node.sig, block),
                fn_span(&node.sig, block),
            );
        }
        visit::visit_trait_item_fn(self, node);
    }

    fn visit_expr_closure(&mut self, node: &'ast ExprClosure) {
        self.push(
            FragmentKind::Closure,
            lower_closure(node),
            line_span(node.span().start(), node.span().end()),
        );
        for input in &node.inputs {
            self.visit_pat(input);
        }
        self.visit_body(&node.body);
    }

    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        self.visit_expr(&node.cond);
        self.visit_block(&node.then_branch);
        if let Some((_, else_branch)) = &node.else_branch {
            self.visit_body(else_branch);
        }
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        self.visit_pat(&node.pat);
        if let Some((_, guard)) = &node.guard {
            self.visit_expr(guard);
        }
        self.visit_body(&node.body);
    }

    fn visit_expr_block(&mut self, node: &'ast ExprBlock) {
        // A *free* block: braces written as an expression or a bare statement,
        // rather than as some construct's body. A body's braces are
        // construct-owned — they are that construct's own shape, already part
        // of the fragment it emits, not a fragment in their own right — so the
        // three expression-typed bodies (`else` branch, match-arm body,
        // closure body) descend through [`Collector::visit_body`] instead.
        // `if`/`loop`/`while`/`for`/function bodies are `syn::Block`s, not
        // block *expressions*, so they never reach here; the flavored blocks
        // (`unsafe`/`async`/`try`/`const`) have their own expression nodes.
        self.push(
            FragmentKind::Block,
            lower_block(&node.block),
            line_span(node.block.span().start(), node.block.span().end()),
        );
        visit::visit_expr_block(self, node);
    }

    fn visit_stmt(&mut self, node: &'ast Stmt) {
        // An item nested inside a body is opaque to lowering (see
        // [`Label::Item`]), so nothing inside it belongs to any fragment;
        // extraction stops at it for the same reason.
        if matches!(node, Stmt::Item(_)) {
            return;
        }
        visit::visit_stmt(self, node);
    }
}

/// The 1-based `(start_line, end_line)` of a function/method — from the `fn`
/// keyword through the body's closing brace.
fn fn_span(sig: &Signature, block: &Block) -> (usize, usize) {
    line_span(sig.fn_token.span().start(), block.span().end())
}

/// A 1-based line span from two locations, guarded so `line_count` can never
/// underflow on a degenerate span.
fn line_span(start: LineColumn, end: LineColumn) -> (usize, usize) {
    let (start, end) = (line_of(start), line_of(end));
    if end < start {
        (start, start)
    } else {
        (start, end)
    }
}

/// A 1-based line number from a `proc-macro2` location.
///
/// The `span-locations` feature (enabled in `Cargo.toml`) makes lines real and
/// 1-based here. Should locations ever be unavailable, `proc-macro2` reports
/// line 0; we deterministically fall back to line 1 so downstream line math
/// stays well-defined.
fn line_of(lc: LineColumn) -> usize {
    if lc.line == 0 {
        1
    } else {
        lc.line
    }
}

/// Build an [`Error::Parse`] whose message folds in the `syn` error location.
fn parse_error(path: &str, err: &syn::Error) -> Error {
    let start = err.span().start();
    Error::Parse {
        path: path.to_string(),
        message: format!("{err} at line {} column {}", start.line, start.column),
    }
}

// --- Lowering ---------------------------------------------------------------

/// Lower a function/method signature + body to a normalized tree.
fn lower_fn(sig: &Signature, block: &Block) -> NormTree {
    let params = NormTree::new(
        Label::Params,
        sig.inputs
            .iter()
            .map(|_| NormTree::leaf(Label::Param))
            .collect(),
    );
    let mut children = vec![params];
    if matches!(sig.output, syn::ReturnType::Type(..)) {
        children.push(NormTree::leaf(Label::ReturnType));
    }
    children.push(lower_block(block));
    NormTree::new(Label::Function, children)
}

/// Lower an `impl` block body: one child per method, in source order.
///
/// Associated consts and types carry only names and types, both of which the
/// normalization contract erases, so they contribute no shape and are skipped.
fn lower_impl(imp: &ItemImpl) -> NormTree {
    NormTree::new(
        Label::Impl,
        imp.items
            .iter()
            .filter_map(|member| match member {
                ImplItem::Fn(f) => Some(lower_fn(&f.sig, &f.block)),
                _ => None,
            })
            .collect(),
    )
}

/// Lower a closure: its input patterns, then its body.
fn lower_closure(e: &ExprClosure) -> NormTree {
    let mut children: Vec<NormTree> = e.inputs.iter().map(lower_pat).collect();
    children.push(lower_expr(&e.body));
    NormTree::new(
        Label::Closure {
            inputs: e.inputs.len(),
            capture: e.capture.is_some(),
        },
        children,
    )
}

/// Lower a `{ .. }` block of the given flavor: one child per statement, in order.
fn lower_block_of(kind: BlockKind, block: &Block) -> NormTree {
    NormTree::new(
        Label::Block(kind),
        block.stmts.iter().map(lower_stmt).collect(),
    )
}

/// Lower a plain `{ .. }` block.
fn lower_block(block: &Block) -> NormTree {
    lower_block_of(BlockKind::Plain, block)
}

/// Lower a single statement.
fn lower_stmt(stmt: &Stmt) -> NormTree {
    match stmt {
        Stmt::Local(local) => {
            let mut children = vec![lower_pat(&local.pat)];
            if let Some(init) = &local.init {
                children.push(lower_expr(&init.expr));
                if let Some((_, diverge)) = &init.diverge {
                    children.push(lower_expr(diverge));
                }
            }
            NormTree::new(Label::Let, children)
        }
        Stmt::Expr(expr, _) => lower_expr(expr),
        Stmt::Macro(m) => NormTree::leaf(macro_label(&m.mac)),
        // A nested item (e.g. an inner `fn`) is not part of the enclosing
        // body's shape, so its interior is not lowered. It is also not
        // extracted as a fragment of its own — `Collector::visit_stmt` stops
        // at it — and that is a non-goal, not deferred work. (T8's EXTENDED
        // granularity covers impl bodies, closures and free blocks; it does
        // not cover in-body nested items.)
        Stmt::Item(_) => NormTree::leaf(Label::Item),
    }
}

/// Lower an expression, canonicalizing identifiers/literals and preserving
/// structure (control flow, operators, argument arity, statement order,
/// mutability, block flavor, pattern shape).
fn lower_expr(expr: &Expr) -> NormTree {
    match expr {
        Expr::Array(e) => node(Label::Array, e.elems.iter()),
        Expr::Assign(e) => NormTree::new(
            Label::Assign,
            vec![lower_expr(&e.left), lower_expr(&e.right)],
        ),
        Expr::Async(e) => lower_block_of(
            BlockKind::Async {
                capture: e.capture.is_some(),
            },
            &e.block,
        ),
        Expr::Await(e) => NormTree::new(Label::Await, vec![lower_expr(&e.base)]),
        Expr::Binary(e) => NormTree::new(
            Label::Binary(bin_op(&e.op)),
            vec![lower_expr(&e.left), lower_expr(&e.right)],
        ),
        Expr::Block(e) => lower_block(&e.block),
        Expr::Break(e) => NormTree::new(Label::Break, opt_child(e.expr.as_deref())),
        Expr::Call(e) => {
            let mut children = vec![lower_expr(&e.func)];
            children.extend(e.args.iter().map(lower_expr));
            NormTree::new(Label::Call, children)
        }
        Expr::Cast(e) => NormTree::new(Label::Cast, vec![lower_expr(&e.expr)]),
        Expr::Closure(e) => lower_closure(e),
        Expr::Const(e) => lower_block_of(BlockKind::Const, &e.block),
        Expr::Continue(_) => NormTree::leaf(Label::Continue),
        Expr::Field(e) => NormTree::new(Label::FieldAccess, vec![lower_expr(&e.base)]),
        Expr::ForLoop(e) => NormTree::new(
            Label::For,
            vec![lower_pat(&e.pat), lower_expr(&e.expr), lower_block(&e.body)],
        ),
        Expr::Group(e) => lower_expr(&e.expr),
        Expr::If(e) => {
            let mut children = vec![lower_expr(&e.cond), lower_block(&e.then_branch)];
            if let Some((_, else_branch)) = &e.else_branch {
                children.push(lower_expr(else_branch));
            }
            NormTree::new(Label::If, children)
        }
        Expr::Index(e) => NormTree::new(
            Label::Index,
            vec![lower_expr(&e.expr), lower_expr(&e.index)],
        ),
        Expr::Infer(_) => NormTree::leaf(Label::Infer),
        Expr::Let(e) => NormTree::new(Label::Let, vec![lower_pat(&e.pat), lower_expr(&e.expr)]),
        Expr::Lit(_) => NormTree::leaf(Label::Literal),
        Expr::Loop(e) => NormTree::new(Label::Loop, vec![lower_block(&e.body)]),
        Expr::Macro(e) => NormTree::leaf(macro_label(&e.mac)),
        Expr::Match(e) => {
            let mut children = vec![lower_expr(&e.expr)];
            children.extend(e.arms.iter().map(lower_arm));
            NormTree::new(Label::Match, children)
        }
        Expr::MethodCall(e) => {
            let mut children = vec![lower_expr(&e.receiver)];
            children.extend(e.args.iter().map(lower_expr));
            NormTree::new(Label::MethodCall, children)
        }
        Expr::Paren(e) => lower_expr(&e.expr),
        Expr::Path(_) => NormTree::leaf(Label::Path),
        Expr::Range(e) => NormTree::new(
            Label::Range {
                kind: range_kind(&e.limits),
                has_start: e.start.is_some(),
                has_end: e.end.is_some(),
            },
            range_bounds(e.start.as_deref(), e.end.as_deref()),
        ),
        Expr::RawAddr(e) => NormTree::new(
            Label::RawAddr(pointer_mutability(&e.mutability)),
            vec![lower_expr(&e.expr)],
        ),
        Expr::Reference(e) => NormTree::new(
            Label::Reference(mutability(e.mutability.is_some())),
            vec![lower_expr(&e.expr)],
        ),
        Expr::Repeat(e) => NormTree::new(
            Label::ArrayRepeat,
            vec![lower_expr(&e.expr), lower_expr(&e.len)],
        ),
        Expr::Return(e) => NormTree::new(Label::Return, opt_child(e.expr.as_deref())),
        Expr::Struct(e) => {
            let mut children: Vec<NormTree> =
                e.fields.iter().map(|f| lower_expr(&f.expr)).collect();
            if let Some(rest) = &e.rest {
                children.push(lower_expr(rest));
            }
            NormTree::new(
                Label::Struct {
                    rest: e.rest.is_some(),
                },
                children,
            )
        }
        Expr::Try(e) => NormTree::new(Label::Try, vec![lower_expr(&e.expr)]),
        Expr::TryBlock(e) => lower_block_of(BlockKind::Try, &e.block),
        Expr::Tuple(e) => node(Label::Tuple, e.elems.iter()),
        Expr::Unary(e) => NormTree::new(Label::Unary(un_op(&e.op)), vec![lower_expr(&e.expr)]),
        Expr::Unsafe(e) => lower_block_of(BlockKind::Unsafe, &e.block),
        Expr::While(e) => NormTree::new(
            Label::While,
            vec![lower_expr(&e.cond), lower_block(&e.body)],
        ),
        Expr::Yield(e) => NormTree::new(Label::Yield, opt_child(e.expr.as_deref())),
        // `Expr::Verbatim` carries tokens `syn` could not parse, and `syn::Expr`
        // is `#[non_exhaustive]` so a future edition may add variants. Neither
        // exposes reachable sub-expressions, so both lower to an opaque leaf.
        _ => NormTree::leaf(Label::Other),
    }
}

/// Lower a pattern: shape and arity are preserved; binding identifiers, paths
/// and literal values are normalized away.
fn lower_pat(pat: &Pat) -> NormTree {
    match pat {
        Pat::Const(p) => lower_block_of(BlockKind::Const, &p.block),
        Pat::Ident(p) => NormTree::new(
            Label::PatBinding {
                by_ref: p.by_ref.is_some(),
                mutable: p.mutability.is_some(),
            },
            p.subpat
                .iter()
                .map(|(_, sub)| lower_pat(sub))
                .collect::<Vec<_>>(),
        ),
        Pat::Lit(_) => NormTree::leaf(Label::PatLiteral),
        Pat::Macro(p) => NormTree::leaf(macro_label(&p.mac)),
        Pat::Or(p) => NormTree::new(Label::PatOr, p.cases.iter().map(lower_pat).collect()),
        Pat::Paren(p) => lower_pat(&p.pat),
        Pat::Path(_) => NormTree::leaf(Label::PatPath),
        Pat::Range(p) => NormTree::new(
            Label::PatRange {
                kind: range_kind(&p.limits),
                has_start: p.start.is_some(),
                has_end: p.end.is_some(),
            },
            range_bounds(p.start.as_deref(), p.end.as_deref()),
        ),
        Pat::Reference(p) => NormTree::new(
            Label::PatReference(mutability(p.mutability.is_some())),
            vec![lower_pat(&p.pat)],
        ),
        Pat::Rest(_) => NormTree::leaf(Label::PatRest),
        Pat::Slice(p) => NormTree::new(Label::PatSlice, p.elems.iter().map(lower_pat).collect()),
        Pat::Struct(p) => NormTree::new(
            Label::PatStruct {
                rest: p.rest.is_some(),
            },
            p.fields.iter().map(|f| lower_pat(&f.pat)).collect(),
        ),
        Pat::Tuple(p) => NormTree::new(Label::PatTuple, p.elems.iter().map(lower_pat).collect()),
        Pat::TupleStruct(p) => NormTree::new(
            Label::PatTupleStruct,
            p.elems.iter().map(lower_pat).collect(),
        ),
        Pat::Type(p) => NormTree::new(Label::PatType, vec![lower_pat(&p.pat)]),
        Pat::Wild(_) => NormTree::leaf(Label::PatWild),
        // `Pat::Verbatim` (unparsed tokens) and any future `#[non_exhaustive]`
        // variant: opaque leaf, no reachable sub-patterns.
        _ => NormTree::leaf(Label::Other),
    }
}

/// Lower a match arm: the pattern, an optional guard, then the body.
fn lower_arm(arm: &syn::Arm) -> NormTree {
    let mut children = vec![lower_pat(&arm.pat)];
    if let Some((_, guard)) = &arm.guard {
        children.push(lower_expr(guard));
    }
    children.push(lower_expr(&arm.body));
    NormTree::new(Label::MatchArm, children)
}

/// The children of a range form — whichever of its two bounds are present.
fn range_bounds(start: Option<&Expr>, end: Option<&Expr>) -> Vec<NormTree> {
    let mut children = opt_child(start);
    children.extend(opt_child(end));
    children
}

/// Build a node whose children are the lowered items of an iterator of exprs.
fn node<'a, I>(label: Label, exprs: I) -> NormTree
where
    I: Iterator<Item = &'a Expr>,
{
    NormTree::new(label, exprs.map(lower_expr).collect())
}

/// Zero-or-one child from an optional expression.
fn opt_child(expr: Option<&Expr>) -> Vec<NormTree> {
    expr.map(lower_expr).into_iter().collect()
}

/// The label for a macro invocation. The token stream is deliberately not
/// parsed (see the `tree` module's "Known limitation" note); only the delimiter
/// and whether the invocation is empty survive.
fn macro_label(mac: &Macro) -> Label {
    let delimiter = match mac.delimiter {
        MacroDelimiter::Paren(_) => Delimiter::Paren,
        MacroDelimiter::Brace(_) => Delimiter::Brace,
        MacroDelimiter::Bracket(_) => Delimiter::Bracket,
    };
    Label::Macro {
        delimiter,
        empty: mac.tokens.is_empty(),
    }
}

/// Map an optional `mut` token to the structural mutability marker.
fn mutability(is_mut: bool) -> Mutability {
    if is_mut {
        Mutability::Mutable
    } else {
        Mutability::Immutable
    }
}

/// Map `&raw const` / `&raw mut` to the structural mutability marker.
fn pointer_mutability(m: &PointerMutability) -> Mutability {
    match m {
        PointerMutability::Const(_) => Mutability::Immutable,
        PointerMutability::Mut(_) => Mutability::Mutable,
    }
}

/// Map `..` / `..=` to the structural range kind.
fn range_kind(limits: &RangeLimits) -> RangeKind {
    match limits {
        RangeLimits::HalfOpen(_) => RangeKind::HalfOpen,
        RangeLimits::Closed(_) => RangeKind::Closed,
    }
}

/// Canonical token for a binary operator (preserved — not a Type-2 rename).
fn bin_op(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add(_) => "+",
        BinOp::Sub(_) => "-",
        BinOp::Mul(_) => "*",
        BinOp::Div(_) => "/",
        BinOp::Rem(_) => "%",
        BinOp::And(_) => "&&",
        BinOp::Or(_) => "||",
        BinOp::BitXor(_) => "^",
        BinOp::BitAnd(_) => "&",
        BinOp::BitOr(_) => "|",
        BinOp::Shl(_) => "<<",
        BinOp::Shr(_) => ">>",
        BinOp::Eq(_) => "==",
        BinOp::Lt(_) => "<",
        BinOp::Le(_) => "<=",
        BinOp::Ne(_) => "!=",
        BinOp::Ge(_) => ">=",
        BinOp::Gt(_) => ">",
        BinOp::AddAssign(_) => "+=",
        BinOp::SubAssign(_) => "-=",
        BinOp::MulAssign(_) => "*=",
        BinOp::DivAssign(_) => "/=",
        BinOp::RemAssign(_) => "%=",
        BinOp::BitXorAssign(_) => "^=",
        BinOp::BitAndAssign(_) => "&=",
        BinOp::BitOrAssign(_) => "|=",
        BinOp::ShlAssign(_) => "<<=",
        BinOp::ShrAssign(_) => ">>=",
        _ => "?",
    }
}

/// Canonical token for a unary operator.
fn un_op(op: &UnOp) -> &'static str {
    match op {
        UnOp::Deref(_) => "*",
        UnOp::Not(_) => "!",
        UnOp::Neg(_) => "-",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_ok(source: &str) -> Vec<Analyzed> {
        extract("test.rs", source).expect("source should parse")
    }

    /// The normalized tree of `fn f() { <body> }` — the unit of the inequality
    /// tests below. Bodies containing a closure or a free block now also yield
    /// nested fragments (T8), so the enclosing function is selected by kind.
    fn body_tree(body: &str) -> NormTree {
        let source = format!("fn f() {{ {body} }}");
        let extracted = extract("test.rs", &source).expect("body should parse");
        let mut functions: Vec<Analyzed> = extracted
            .into_iter()
            .filter(|e| e.fragment.kind == FragmentKind::Function)
            .collect();
        assert_eq!(functions.len(), 1, "expected exactly one function fragment");
        functions.remove(0).tree
    }

    /// Assert every listed body lowers to a tree distinct from all the others.
    fn assert_all_distinct(bodies: &[&str]) {
        let trees: Vec<NormTree> = bodies.iter().map(|b| body_tree(b)).collect();
        for (i, left) in trees.iter().enumerate() {
            for (j, right) in trees.iter().enumerate().skip(i + 1) {
                assert_ne!(
                    left, right,
                    "`{}` and `{}` must not lower to equal trees",
                    bodies[i], bodies[j]
                );
            }
        }
    }

    #[test]
    fn extracts_free_functions_methods_and_the_impl_block() {
        let source = "\
fn a() { let x = 1; }
fn b(y: u32) -> u32 { y + 1 }
struct S;
impl S {
    fn m1(&self) {}
    fn m2(&self, z: u32) -> u32 { z }
}
";
        let extracted = extract_ok(source);
        let kinds: Vec<FragmentKind> = extracted.iter().map(|e| e.fragment.kind).collect();
        // The `impl` block precedes its own methods: it starts on an earlier
        // line, and the canonical key sorts on the span (T8).
        assert_eq!(
            kinds,
            vec![
                FragmentKind::Function,
                FragmentKind::Function,
                FragmentKind::ImplBlock,
                FragmentKind::Method,
                FragmentKind::Method,
            ]
        );

        // Plausible, 1-based, non-decreasing line spans within the file.
        for e in &extracted {
            assert!(e.fragment.start_line >= 1);
            assert!(e.fragment.end_line >= e.fragment.start_line);
            assert_eq!(
                e.fragment.line_count,
                e.fragment.end_line - e.fragment.start_line + 1
            );
        }
        // `fn a` starts on line 1; the last method ends deeper in the file.
        assert_eq!(extracted[0].fragment.start_line, 1);
        assert!(extracted[4].fragment.end_line >= 6);
        // The `impl` block spans its whole body, methods included.
        assert_eq!(
            (
                extracted[2].fragment.start_line,
                extracted[2].fragment.end_line
            ),
            (4, 7)
        );
    }

    #[test]
    fn type2_rename_lowers_to_equal_trees() {
        // Same structure, different identifiers and literal values.
        let a = extract_ok("fn one() { let total = 10; let result = total + 2; result }");
        let b = extract_ok("fn two() { let sum = 99; let out = sum + 7; out }");
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert_eq!(
            a[0].tree, b[0].tree,
            "renamed (Type-2) clones must lower to equal trees"
        );
    }

    #[test]
    fn structurally_different_functions_lower_to_unequal_trees() {
        let looping = extract_ok("fn f() { for _ in 0..3 {} }");
        let binding = extract_ok("fn g() { let x = 1; }");
        assert_ne!(looping[0].tree, binding[0].tree);
    }

    #[test]
    fn node_count_matches_tree() {
        let extracted = extract_ok("fn f(a: u32) -> u32 { a + 1 }");
        assert_eq!(extracted.len(), 1);
        assert_eq!(
            extracted[0].fragment.node_count,
            extracted[0].tree.node_count()
        );
    }

    #[test]
    fn extraction_order_is_deterministic_by_canonical_key() {
        let source = "\
fn first() {}
fn second() {}
fn third() {}
";
        let starts: Vec<usize> = extract_ok(source)
            .iter()
            .map(|e| e.fragment.start_line)
            .collect();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
    }

    #[test]
    fn malformed_source_is_parse_error_with_location() {
        let err = extract("bad.rs", "fn broken( {").expect_err("should fail to parse");
        match err {
            Error::Parse { path, message } => {
                assert_eq!(path, "bad.rs");
                assert!(
                    message.contains("line") && message.contains("column"),
                    "message should carry location, got: {message}"
                );
            }
            other => panic!("expected Error::Parse, got {other:?}"),
        }
    }

    #[test]
    fn extracts_trait_default_trait_impl_and_inline_mod_bodies() {
        let source = "\
trait T {
    fn required(&self);
    fn defaulted(&self) { let a = 1; }
}
struct S;
impl T for S {
    fn required(&self) { let b = 2; }
}
mod inner {
    fn nested() {}
    mod deeper {
        fn deepest() {}
    }
}
";
        let extracted = extract_ok(source);
        // trait default body, the trait `impl` block and its method, and both
        // inline-mod functions — the `required` *declaration* (no body) is not
        // a fragment.
        let kinds: Vec<FragmentKind> = extracted.iter().map(|e| e.fragment.kind).collect();
        assert_eq!(
            kinds,
            vec![
                FragmentKind::Method,
                FragmentKind::ImplBlock,
                FragmentKind::Method,
                FragmentKind::Function,
                FragmentKind::Function,
            ]
        );
    }

    // --- T8: nested (EXTENDED) extraction ------------------------------------

    #[test]
    fn extracts_closures_and_free_blocks_nested_inside_their_parents() {
        let source = "\
fn outer() {
    let doubler = |x| {
        x * 2
    };
    let scoped = {
        let inner = |y| y;
        inner(1)
    };
}
";
        let extracted = extract_ok(source);
        let kinds: Vec<FragmentKind> = extracted.iter().map(|e| e.fragment.kind).collect();
        // Outer function, the first closure, the free block, then the closure
        // written inside that block — ordered by `(start_line, end_line)`.
        assert_eq!(
            kinds,
            vec![
                FragmentKind::Function,
                FragmentKind::Closure,
                FragmentKind::Block,
                FragmentKind::Closure,
            ]
        );

        // Nested fragments overlap their parents by construction at T8 —
        // containment-dedup (T9) is what removes the redundancy later.
        let spans: Vec<(usize, usize)> = extracted
            .iter()
            .map(|e| (e.fragment.start_line, e.fragment.end_line))
            .collect();
        assert_eq!(spans, vec![(1, 9), (2, 4), (5, 8), (6, 6)]);
    }

    #[test]
    fn a_control_flow_body_is_not_a_free_block() {
        // `if` / `loop` / `for` / `while` / `match`-arm bodies are part of the
        // enclosing construct's shape, so only the function is a fragment.
        let source = "\
fn f(v: bool) {
    if v { g(); } else { h(); }
    loop { g(); }
    for x in v { g(); }
    while v { g(); }
    match v { true => { g(); } false => {} }
}
";
        let kinds: Vec<FragmentKind> = extract_ok(source).iter().map(|e| e.fragment.kind).collect();
        assert_eq!(kinds, vec![FragmentKind::Function]);
    }

    #[test]
    fn nested_fragment_node_counts_match_their_own_trees_and_fit_their_parents() {
        let source = "\
fn outer() {
    let f = |x| x + 1;
    { let y = 2; }
}
struct S;
impl S {
    fn m(&self) { let g = |z| z; }
}
";
        let extracted = extract_ok(source);
        for e in &extracted {
            assert_eq!(
                e.fragment.node_count,
                e.tree.node_count(),
                "node count must come from the fragment's own tree"
            );
        }

        let by_kind = |kind: FragmentKind| -> Vec<usize> {
            extracted
                .iter()
                .filter(|e| e.fragment.kind == kind)
                .map(|e| e.fragment.node_count)
                .collect()
        };
        // `Closure{1,false}(PatBinding, Binary(Path, Literal))` = 5 nodes;
        // `Block(Let(PatBinding, Literal))` = 4; the method's closure
        // `Closure(PatBinding, Path)` = 3.
        assert_eq!(by_kind(FragmentKind::Closure), vec![5, 3]);
        assert_eq!(by_kind(FragmentKind::Block), vec![4]);
        // Every nested fragment is strictly smaller than the parent it actually
        // sits in: the first closure and the free block are inside `outer`, the
        // second closure is inside `S::m`.
        let function = by_kind(FragmentKind::Function);
        let method = by_kind(FragmentKind::Method);
        assert_eq!((function.len(), method.len()), (1, 1));
        assert!(by_kind(FragmentKind::Closure)[0] < function[0]);
        assert!(by_kind(FragmentKind::Block)[0] < function[0]);
        assert!(by_kind(FragmentKind::Closure)[1] < method[0]);
        // The `impl` block wraps its single method's tree: `Impl(Function(..))`.
        let impl_block = by_kind(FragmentKind::ImplBlock);
        assert_eq!(impl_block, vec![by_kind(FragmentKind::Method)[0] + 1]);
    }

    #[test]
    fn an_impl_block_lowers_to_its_methods_in_source_order() {
        let source = "\
struct S;
impl S {
    const N: u32 = 1;
    fn a(&self) { let x = 1; }
    fn b(&self) {}
}
";
        let extracted = extract_ok(source);
        let block = extracted
            .iter()
            .find(|e| e.fragment.kind == FragmentKind::ImplBlock)
            .expect("the impl block is a fragment");
        let methods: Vec<&NormTree> = extracted
            .iter()
            .filter(|e| e.fragment.kind == FragmentKind::Method)
            .map(|e| &e.tree)
            .collect();

        // Associated consts carry only erased names/types and contribute no
        // shape, so the block's children are exactly its two method trees.
        assert_eq!(block.tree.label, Label::Impl);
        assert_eq!(block.tree.children.len(), 2);
        assert_eq!(&block.tree.children[0], methods[0]);
        assert_eq!(&block.tree.children[1], methods[1]);
    }

    #[test]
    fn an_item_nested_in_a_body_is_not_descended_into() {
        // The lowering treats an in-body item as an opaque leaf, so nothing
        // inside it belongs to any fragment — extraction stops there too.
        let source = "\
fn outer() {
    fn inner() { let f = |x| x; }
    let g = |y| y;
}
";
        let kinds: Vec<FragmentKind> = extract_ok(source).iter().map(|e| e.fragment.kind).collect();
        assert_eq!(kinds, vec![FragmentKind::Function, FragmentKind::Closure]);
    }

    // --- Normalization inequality guards (R3) -------------------------------
    //
    // Each of these pins a structural distinction that changes program meaning
    // and must therefore change the tree. Together with
    // `type2_rename_lowers_to_equal_trees` they bracket the contract from both
    // sides: names/literal values are erased, structure is not.

    #[test]
    fn binary_operators_are_distinct() {
        assert_all_distinct(&["let z = a + b;", "let z = a - b;"]);
    }

    #[test]
    fn array_list_and_array_repeat_are_distinct() {
        assert_all_distinct(&["let z = [x, y];", "let z = [x; y];"]);
    }

    #[test]
    fn range_limits_are_distinct() {
        assert_all_distinct(&["let z = a..b;", "let z = a..=b;"]);
    }

    #[test]
    fn range_bound_presence_is_distinct() {
        // One-sided ranges carry one child each; the label — not the child
        // count — is what keeps them apart.
        assert_all_distinct(&["let z = ..b;", "let z = a..;", "let z = a..b;"]);
        assert_all_distinct(&[
            "match v { ..B => x, }",
            "match v { A.. => x, }",
            "match v { A..B => x, }",
            "match v { ..=B => x, }",
            "match v { A..=B => x, }",
        ]);
    }

    #[test]
    fn reference_mutability_is_distinct() {
        assert_all_distinct(&["let z = &x;", "let z = &mut x;"]);
    }

    #[test]
    fn block_flavors_are_distinct() {
        assert_all_distinct(&[
            "let z = { x };",
            "let z = async { x };",
            "let z = async move { x };",
            "let z = unsafe { x };",
            "let z = try { x };",
            "let z = const { x };",
        ]);
    }

    #[test]
    fn closure_arity_and_capture_are_distinct() {
        assert_all_distinct(&["let z = |x| x;", "let z = |x, y| x;", "let z = move |x| x;"]);
    }

    #[test]
    fn let_pattern_shapes_are_distinct() {
        assert_all_distinct(&[
            "let x = v;",
            "let (x, y) = v;",
            "let [x, y] = v;",
            "let S { x, y } = v;",
            "let &x = v;",
            "let _ = v;",
            "let ref mut x = v;",
        ]);
    }

    #[test]
    fn match_arm_and_loop_pattern_shapes_are_distinct() {
        assert_all_distinct(&[
            "match v { x => x, }",
            "match v { (x, y) => x, }",
            "match v { V(x) => x, }",
            "match v { 1 | 2 => x, }",
            "match v { [x, ..] => x, }",
        ]);
        assert_all_distinct(&["for x in v {}", "for (x, y) in v {}", "for _ in v {}"]);
        assert_all_distinct(&["if let x = v {}", "if let (x, y) = v {}"]);
        assert_all_distinct(&["while let x = v {}", "while let (x, y) = v {}"]);
    }

    #[test]
    fn previously_opaque_forms_are_now_distinct() {
        // `yield` and a const block used to collapse into the single opaque
        // `Other` leaf; they are structural labels now.
        assert_all_distinct(&["yield x;", "const { x };", "x;"]);
    }

    #[test]
    fn macro_invocations_keep_delimiter_and_emptiness() {
        // Macro token streams stay unparsed (documented limitation), but the
        // cheap structural discriminators survive.
        assert_all_distinct(&["m!();", "m![];", "m!{}", "m!(x);"]);
    }

    #[test]
    fn type2_rename_still_erases_names_and_literal_values() {
        // The other side of the contract: differing identifiers, literal values
        // and field/method names must NOT change the tree.
        assert_eq!(
            body_tree("let a = 1; b.field.method(a, 2);"),
            body_tree("let zzz = 987; other.thing.call(zzz, 654);")
        );
    }
}
