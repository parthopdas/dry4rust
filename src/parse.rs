//! Parse adapter: `syn` → normalized label tree + fragment extraction.
//! Confines the `syn` / `proc-macro2` dependencies to this module.
//!
//! S1 extracts **free functions and methods** (inherent-impl, trait-impl, and
//! trait default-method bodies). `impl` bodies, closures, and free `{}` blocks
//! are S2 (T8) and are *not* extracted here — though a closure appearing inside
//! a function body is still lowered as part of that function's tree.
//!
//! Lowering canonicalizes identifiers/literals and preserves structure; see
//! [`crate::tree`] for the normalization contract.

use proc_macro2::LineColumn;
use syn::spanned::Spanned;
use syn::{
    BinOp, Block, Expr, ImplItem, Item, Macro, MacroDelimiter, Pat, PointerMutability, RangeLimits,
    Signature, Stmt, TraitItem, UnOp,
};

use crate::error::{Error, Result};
use crate::model::{Analyzed, Fragment, FragmentKind};
use crate::tree::{BlockKind, Delimiter, Label, Mutability, NormTree, RangeKind};

/// Parse `source` (the contents of the `/`-normalized `path`) and extract every
/// S1 fragment (free functions + methods), each paired with its normalized tree.
///
/// Results are ordered deterministically by `Fragment::canonical_key`. A parse
/// failure is mapped to [`Error::Parse`] with the `syn` error's line/column
/// folded into the message (the variant carries only a `String`), so location
/// survives (N2).
pub(crate) fn extract(path: &str, source: &str) -> Result<Vec<Analyzed>> {
    let file = syn::parse_file(source).map_err(|err| parse_error(path, &err))?;
    let mut out = Vec::new();
    collect_items(path, &file.items, &mut out);
    out.sort_by(|a, b| a.fragment.canonical_key().cmp(&b.fragment.canonical_key()));
    Ok(out)
}

/// Recursively collect fragments from a list of items (descending into inline
/// modules so `mod m { fn .. }` is covered).
fn collect_items(path: &str, items: &[Item], out: &mut Vec<Analyzed>) {
    for item in items {
        match item {
            Item::Fn(f) => out.push(build(path, FragmentKind::Function, &f.sig, &f.block)),
            Item::Impl(imp) => {
                for member in &imp.items {
                    if let ImplItem::Fn(m) = member {
                        out.push(build(path, FragmentKind::Method, &m.sig, &m.block));
                    }
                }
            }
            Item::Trait(tr) => {
                for member in &tr.items {
                    if let TraitItem::Fn(m) = member {
                        // Only trait methods with a default body are fragments.
                        if let Some(block) = &m.default {
                            out.push(build(path, FragmentKind::Method, &m.sig, block));
                        }
                    }
                }
            }
            Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    collect_items(path, inner, out);
                }
            }
            _ => {}
        }
    }
}

/// Assemble one [`Analyzed`] from a function/method signature and body.
fn build(path: &str, kind: FragmentKind, sig: &Signature, block: &Block) -> Analyzed {
    let tree = lower_fn(sig, block);
    let (start_line, end_line) = fn_span(sig, block);
    let fragment = Fragment {
        path: path.to_string(),
        start_line,
        end_line,
        node_count: tree.node_count(),
        // N9: `line_count` is the physical span height `end - start + 1` (the
        // value `--min-lines` gates on in T5). Logical LOC was considered for
        // closer dry4go parity but the physical span is unambiguous and stable.
        line_count: end_line - start_line + 1,
        kind,
    };
    Analyzed { fragment, tree }
}

/// The 1-based `(start_line, end_line)` of a function/method — from the `fn`
/// keyword through the body's closing brace.
fn fn_span(sig: &Signature, block: &Block) -> (usize, usize) {
    let start = line_of(sig.fn_token.span().start());
    let end = line_of(block.span().end());
    // Guard against any degenerate span so `line_count` never underflows.
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
        // extracted as a fragment of its own — `collect_items` does not
        // descend into function bodies — and that is a non-goal, not deferred
        // work. (T8 extends granularity to impl bodies, closures and free
        // blocks; it does not cover in-body nested items.)
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
        Expr::Closure(e) => {
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
    /// tests below.
    fn body_tree(body: &str) -> NormTree {
        let source = format!("fn f() {{ {body} }}");
        let mut extracted = extract("test.rs", &source).expect("body should parse");
        assert_eq!(extracted.len(), 1, "expected exactly one fragment");
        extracted.remove(0).tree
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
    fn extracts_free_functions_and_methods() {
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
        assert_eq!(
            kinds,
            vec![
                FragmentKind::Function,
                FragmentKind::Function,
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
        assert!(extracted[3].fragment.end_line >= 6);
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
        // trait default body, trait-impl method, and both inline-mod functions —
        // the `required` *declaration* (no body) is not a fragment.
        let kinds: Vec<FragmentKind> = extracted.iter().map(|e| e.fragment.kind).collect();
        assert_eq!(
            kinds,
            vec![
                FragmentKind::Method,
                FragmentKind::Method,
                FragmentKind::Function,
                FragmentKind::Function,
            ]
        );
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
