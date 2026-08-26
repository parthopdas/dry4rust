//! Normalized label-tree model and node counting (core; pure, std-only).
//!
//! This is the language-agnostic tree the TED engine (T4) consumes. Fragments
//! are lowered here from `syn` by the `parse` adapter, but this module knows
//! nothing about `syn`: it is pure `std`.
//!
//! # Normalization contract (A6, R3)
//!
//! Each node carries a [`Label`] — a *structural* node-kind drawn from the Rust
//! syntactic categories we preserve (mirroring dry4go's preserved list). The
//! rule is exact and one-directional:
//!
//! > **Only identifiers (paths, local names, field/method/selector names),
//! > *types*, and literal *values* are normalized away. Every other syntactic
//! > distinction — anything that can change what the program means — must
//! > change the tree.**
//!
//! So every path/identifier collapses to [`Label::Path`] and every literal to
//! [`Label::Literal`] regardless of name or value, and types are erased
//! wherever they appear ([`Label::Param`], [`Label::ReturnType`] as
//! presence-only, `as T` casts, [`Label::PatType`]) — this is what lets renamed
//! Type-2 clones lower to *equal* trees — while operators, arity, statement
//! order, control flow, mutability, block flavor, range shape, pattern shape,
//! `fn` qualifiers, receiver form (`self` / `&self` / `&mut self`, taken from
//! the effective self type so `self: &Self` ≡ `&self`; other typed receivers
//! are `Owned`) and the meaning-bearing statement semicolon
//! are all preserved and keep genuinely different code apart.
//!
//! # Known limitation — macros
//!
//! A macro invocation is a leaf: its token stream is not parsed, so two macro
//! calls with the same delimiter and the same emptiness lower identically
//! (`vec![a, b]` vs `vec![c]`). Only the delimiter and whether the token stream
//! is empty are preserved (see [`Label::Macro`]). Parsing macro bodies is out
//! of scope for S1; this is a deliberate, recorded limitation.

/// Whether a reference, raw-address or reference-pattern is mutable.
///
/// `&raw const p` maps to [`Mutability::Immutable`] and `&raw mut p` to
/// [`Mutability::Mutable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mutability {
    /// `&x`, `&raw const x`, `&pat`.
    Immutable,
    /// `&mut x`, `&raw mut x`, `&mut pat`.
    Mutable,
}

/// Which end-inclusivity a range form has — `a..b` vs `a..=b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeKind {
    /// `a..b` — end exclusive.
    HalfOpen,
    /// `a..=b` — end inclusive.
    Closed,
}

/// The receiver form of a method — `self` / `&self` / `&mut self` (N14).
///
/// A method with no receiver (an associated function) simply has no
/// [`Label::Receiver`] child, so "no receiver" needs no variant of its own.
/// The form is taken from the receiver's **effective self type** (N73), so
/// `self: &Self` ≡ `&self` and `self: &mut Self` ≡ `&mut self`. `mut self`
/// maps to [`ReceiverForm::Owned`]: like a `mut x` parameter it is a
/// local binding mode, invisible to callers, and parameter binding modes are
/// already erased.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReceiverForm {
    /// `self` (and `mut self`, and a typed receiver over a non-reference type
    /// such as `self: Box<Self>` — the type is erased like every other type).
    Owned,
    /// `&self` (and `self: &Self`).
    Ref,
    /// `&mut self` (and `self: &mut Self`).
    RefMut,
}

/// The flavor of a `{ .. }` block. Flavors change evaluation semantics, so they
/// are preserved rather than collapsed to a bare block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockKind {
    /// A plain `{ .. }` block (including a function body).
    Plain,
    /// `async { .. }` / `async move { .. }`; `capture` is `true` for `move`
    /// (it changes how the future captures its environment, so it is preserved
    /// exactly as a closure's `capture` is).
    Async { capture: bool },
    /// `unsafe { .. }`.
    Unsafe,
    /// `try { .. }`.
    Try,
    /// `const { .. }`.
    Const,
}

/// The delimiter a macro invocation was written with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Delimiter {
    /// `m!( .. )`.
    Paren,
    /// `m!{ .. }`.
    Brace,
    /// `m![ .. ]`.
    Bracket,
}

/// A binary operator, one variant per operator the language spells differently
/// (N27).
///
/// Operators are *not* Type-2 renames: `a + b` and `a - b` mean different
/// things, so each keeps its own label. [`BinOpKind::Other`] is the residual
/// for an operator a future `syn` may add (`syn::BinOp` is `#[non_exhaustive]`);
/// every operator `syn` 2.0 defines today maps to a variant of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinOpKind {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    BitXor,
    BitAnd,
    BitOr,
    Shl,
    Shr,
    Eq,
    Lt,
    Le,
    Ne,
    Ge,
    Gt,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    RemAssign,
    BitXorAssign,
    BitAndAssign,
    BitOrAssign,
    ShlAssign,
    ShrAssign,
    /// An operator unknown to this version — see the type-level note.
    Other,
}

/// A unary operator. As with [`BinOpKind`], [`UnOpKind::Other`] is the residual
/// for a future `syn` variant; `*`, `!` and `-` all have labels of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnOpKind {
    /// `*expr`.
    Deref,
    /// `!expr`.
    Not,
    /// `-expr`.
    Neg,
    /// An operator unknown to this version — see the type-level note.
    Other,
}

/// A structural node kind — the preserved Rust syntactic categories.
///
/// Identifiers and literals are canonicalized to [`Label::Path`] /
/// [`Label::Literal`] and types are erased; structure (arity, statement order, control flow,
/// operators, mutability, block flavor, pattern shape) is preserved. Free
/// functions and methods share [`Label::Function`], so an associated function
/// and an equivalent free function lower to the same shape — a *method* differs
/// only through its [`Label::Receiver`] parameter — and the
/// `model::FragmentKind` distinction is carried separately.
///
/// Every payload here is `Copy` (flags, counts and the small `Copy` enums
/// above), so the label itself is `Copy` (N75): `ted`'s per-fragment flatten is
/// a memcpy rather than a clone. Adding a heap-carrying payload would take that
/// away — and would be a normalization change (A6/R3) in the first place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Label {
    // --- Fragment / signature shape ---
    /// A function or method body together with its signature shape. The `fn`
    /// qualifiers are preserved (N14): they change what the item *is* to its
    /// callers (an `async fn` returns a future, a `const fn` is callable in
    /// const context, an `unsafe fn` shifts the proof obligation), so two
    /// functions differing only in a qualifier are not clones of each other.
    Function {
        is_async: bool,
        is_const: bool,
        is_unsafe: bool,
    },
    /// An `impl` block body; children are its methods' [`Label::Function`]
    /// nodes in source order (associated consts and types carry only erased
    /// names/types and so contribute no shape).
    Impl,
    /// The parameter list; its children are one [`Label::Param`] per input.
    Params,
    /// A single parameter (identifier and type normalized away — arity only).
    Param,
    /// The `self` parameter of a method, carrying its form (N14). It occupies
    /// the receiver's slot in [`Label::Params`], so a method and a free
    /// function of the same arity no longer lower alike.
    Receiver(ReceiverForm),
    /// Present iff the signature declares an explicit return type.
    ReturnType,

    // --- Blocks & statements ---
    /// A block; the flavor (plain / `async` / `unsafe` / `try` / `const`) is
    /// preserved. Children are its statements in source order.
    ///
    /// `tail` records whether the block ends in a **tail expression** — a final
    /// statement written without its terminating semicolon (N15). That is the
    /// semicolon that carries meaning: `{ x }` evaluates to `x` while `{ x; }`
    /// evaluates to `()`. A semicolon earlier in the block is not a
    /// distinction — a non-final statement expression is unit-typed either way
    /// (and, unless it is block-like, cannot be written without one at all).
    ///
    /// # Known limitation — `tail` is syntactic
    ///
    /// `tail` records how the block was *written*, not what it evaluates to:
    /// `fn f() { if a { b(); } }` and `fn f() { if a { b(); }; }` are
    /// semantically identical yet lower differently. Deciding value-ness needs
    /// type inference, which `parse` does not — and must not — have; this is
    /// the unavoidable mirror of the distinction `tail` exists to keep (N15).
    Block { kind: BlockKind, tail: bool },
    /// A `let` binding; children are the bound pattern then its initializer /
    /// `else` diverge exprs.
    Let,
    /// An item nested inside a body (e.g. an inner `fn`). Its interior is not
    /// lowered — it is not part of the enclosing body's shape. Such an in-body
    /// item is also not extracted as a fragment of its own; that is a non-goal.
    Item,

    // --- Control flow ---
    /// An `if` / `if let`; children are condition, then-block, optional else.
    If,
    /// A `match`; children are the scrutinee then one [`Label::MatchArm`] each.
    Match,
    /// A single match arm; children are the pattern, optional guard, then body.
    MatchArm,
    /// An unconditional `loop`.
    Loop,
    /// A `while` / `while let`.
    While,
    /// A `for` loop; children are the pattern, the iterator expr, the body.
    For,
    /// A `break`, optionally carrying a value expr.
    Break,
    /// A `continue`.
    Continue,
    /// A `return`, optionally carrying a value expr.
    Return,
    /// A `yield`, optionally carrying a value expr.
    Yield,

    // --- Expressions ---
    /// A call `f(..)`; children are the callee then the arguments.
    Call,
    /// A method call `recv.m(..)`; method name normalized away.
    MethodCall,
    /// A field access `recv.field`; field name normalized away.
    FieldAccess,
    /// An index `base[idx]`.
    Index,
    /// An assignment `lhs = rhs` (compound assigns keep their operator).
    Assign,
    /// A binary operation; the operator is preserved.
    Binary(BinOpKind),
    /// A unary operation; the operator is preserved.
    Unary(UnOpKind),
    /// A reference `&expr` / `&mut expr`.
    Reference(Mutability),
    /// A raw-address `&raw const expr` / `&raw mut expr`.
    RawAddr(Mutability),
    /// A cast `expr as T`.
    Cast,
    /// An `.await`.
    Await,
    /// A `?` try.
    Try,
    /// A closure expression. `inputs` is the declared input arity (names and
    /// types normalized away); `capture` is `true` for a `move` closure.
    /// Children are the lowered input patterns then the body.
    Closure { inputs: usize, capture: bool },
    /// A range `a..b` / `a..=b` / `..b` / `a..`; `kind` is the end-inclusivity
    /// and `has_start` / `has_end` record which bounds were written. Children
    /// are the present bounds in source order — the flags are what keeps `..b`
    /// and `a..` (one child each) apart.
    Range {
        kind: RangeKind,
        has_start: bool,
        has_end: bool,
    },
    /// A tuple expression.
    Tuple,
    /// An array list `[a, b, c]`.
    Array,
    /// An array repeat `[value; len]`; children are the value then the length.
    ArrayRepeat,
    /// A struct literal; children are its field value exprs, then the
    /// functional-update base expr when `rest` is set (`S { a, ..base }`).
    Struct { rest: bool },
    /// A macro invocation. Its tokens are **not** parsed — see the module-level
    /// "Known limitation" note; only the delimiter and whether the token stream
    /// is empty are preserved.
    Macro { delimiter: Delimiter, empty: bool },
    /// An inferred `_` expression.
    Infer,
    /// An identifier / path reference (name normalized away).
    Path,
    /// Any literal (value and type normalized away to a single kind).
    Literal,

    // --- Patterns ---
    //
    // Patterns are lowered as structure: shape and arity are preserved, while
    // binding identifiers, paths and literal values are normalized away.
    /// A wildcard pattern `_`.
    PatWild,
    /// A binding `x` / `ref x` / `mut x` / `x @ subpat`; the name is normalized
    /// away. The child, when present, is the `@` sub-pattern.
    PatBinding { by_ref: bool, mutable: bool },
    /// A tuple pattern `(a, b)`.
    PatTuple,
    /// A tuple-struct pattern `V(a, b)`; the path is normalized away.
    PatTupleStruct,
    /// A struct pattern `S { a, b }` / `S { a, .. }`; the path and field names
    /// are normalized away, `rest` records a trailing `..`.
    PatStruct { rest: bool },
    /// A slice pattern `[a, b]`.
    PatSlice,
    /// A reference pattern `&p` / `&mut p`.
    PatReference(Mutability),
    /// An or-pattern `a | b`; children are the alternatives.
    PatOr,
    /// The `..` rest element of a tuple/slice/struct pattern.
    PatRest,
    /// A path pattern (unit struct / variant / const); name normalized away.
    PatPath,
    /// A literal pattern (value normalized away).
    PatLiteral,
    /// A range pattern `a..b` / `a..=b` / `..=b` / `a..`; `kind` is the
    /// end-inclusivity and `has_start` / `has_end` record which bounds were
    /// written. Children are the present bounds in source order.
    PatRange {
        kind: RangeKind,
        has_start: bool,
        has_end: bool,
    },
    /// A type-ascribed pattern `p: T`; the type is normalized away.
    PatType,

    /// Fallback for a syntactic form that is genuinely unknown to us: a future
    /// `#[non_exhaustive]` `syn` variant, or verbatim (unparsed) tokens. Every
    /// variant `syn` 2.0 defines today has a real label above.
    Other,
}

/// A node in the normalized label tree.
///
/// The tree is working state for TED/detect (T4/T5); it is intentionally *not*
/// a field on `model::Fragment` (the parse adapter pairs them via
/// [`crate::parse::Extracted`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormTree {
    /// This node's structural kind.
    pub(crate) label: Label,
    /// Ordered children (source order is significant and preserved).
    pub(crate) children: Vec<NormTree>,
}

impl NormTree {
    /// A leaf node with no children.
    pub(crate) fn leaf(label: Label) -> Self {
        Self {
            label,
            children: Vec::new(),
        }
    }

    /// An internal node with the given ordered children.
    pub(crate) fn new(label: Label, children: Vec<NormTree>) -> Self {
        Self { label, children }
    }

    /// Total number of nodes in this subtree (this node plus all descendants).
    pub(crate) fn node_count(&self) -> usize {
        1 + self
            .children
            .iter()
            .map(NormTree::node_count)
            .sum::<usize>()
    }

    /// Height of this subtree in nodes: a leaf is `1`, and each level of
    /// nesting adds one — the same "counts self" convention as
    /// [`NormTree::node_count`].
    ///
    /// The second axis of per-pair cost (**R9**): Zhang–Shasha costs
    /// `O(n₁·n₂·min(depth₁,leaves₁)·min(depth₂,leaves₂))`, so node count alone
    /// does not price a pair. Reported beside `node_count` as a run statistic
    /// (T14/N107(3), the measurement **D9**'s trigger is stated against) and,
    /// like every counter, never rendered into the report.
    pub(crate) fn depth(&self) -> usize {
        1 + self.children.iter().map(NormTree::depth).max().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_count_counts_self_and_all_descendants() {
        // Function
        // └─ Block
        //    ├─ Let
        //    └─ Return
        let tree = NormTree::new(
            Label::Function {
                is_async: false,
                is_const: false,
                is_unsafe: false,
            },
            vec![NormTree::new(
                Label::Block {
                    kind: BlockKind::Plain,
                    tail: false,
                },
                vec![NormTree::leaf(Label::Let), NormTree::leaf(Label::Return)],
            )],
        );
        assert_eq!(tree.node_count(), 4);
    }

    #[test]
    fn leaf_has_node_count_one() {
        assert_eq!(NormTree::leaf(Label::Literal).node_count(), 1);
    }

    /// Depth is the **longest** root-to-leaf path, not the shortest and not the
    /// child count: the two axes are independent, which is exactly why R9's
    /// cost is not readable off `node_count`. A leaf is `1`, on the same
    /// counts-self convention as `node_count`.
    #[test]
    fn depth_is_the_longest_root_to_leaf_path() {
        assert_eq!(NormTree::leaf(Label::Literal).depth(), 1);

        // Function
        // └─ Block            <- shallow sibling (Let) and a deeper one
        //    ├─ Let
        //    └─ Block
        //       └─ Return
        let tree = NormTree::new(
            Label::Function {
                is_async: false,
                is_const: false,
                is_unsafe: false,
            },
            vec![NormTree::new(
                Label::Block {
                    kind: BlockKind::Plain,
                    tail: false,
                },
                vec![
                    NormTree::leaf(Label::Let),
                    NormTree::new(
                        Label::Block {
                            kind: BlockKind::Plain,
                            tail: false,
                        },
                        vec![NormTree::leaf(Label::Return)],
                    ),
                ],
            )],
        );
        assert_eq!(tree.node_count(), 5);
        assert_eq!(tree.depth(), 4);
    }
}
