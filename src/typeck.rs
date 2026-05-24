//! Lightweight type checking: validate annotations and propagate L1 value
//! types. Consumes a [`Resolution`] from [`crate::resolve`].

use indexmap::IndexMap;

use crate::parse::ast;
use crate::parse::error::ParseError;
use crate::parse::span::Span;
use crate::resolve::{BindingId, BindingKind, Resolution};

/// **M03.2**: integer-kind discriminator. Used by `Ty::Int` and `Value::Int`.
/// `USize` / `ISize` are pinned to 64-bit width for browser determinism
/// (per FR-011); their `min_value`/`max_value` match `U64`/`I64`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)] // Variant names are self-documenting (`I8`, `U16`, ...).
pub enum IntKind {
    I8, I16, I32, I64, I128,
    U8, U16, U32, U64, U128,
    ISize, USize,
}

impl IntKind {
    /// Lowest representable value as `i128` (wide-enough storage for all
    /// variants). For unsigned types this is `0`.
    pub fn min_value(self) -> i128 {
        match self {
            Self::I8 => i8::MIN as i128,
            Self::I16 => i16::MIN as i128,
            Self::I32 => i32::MIN as i128,
            Self::I64 => i64::MIN as i128,
            Self::I128 => i128::MIN,
            Self::U8 | Self::U16 | Self::U32 | Self::U64 | Self::U128 | Self::USize => 0,
            Self::ISize => i64::MIN as i128, // FR-011: isize ≡ i64.
        }
    }

    /// Highest representable value as `i128`.
    pub fn max_value(self) -> i128 {
        match self {
            Self::I8 => i8::MAX as i128,
            Self::I16 => i16::MAX as i128,
            Self::I32 => i32::MAX as i128,
            Self::I64 => i64::MAX as i128,
            Self::I128 => i128::MAX,
            Self::U8 => u8::MAX as i128,
            Self::U16 => u16::MAX as i128,
            Self::U32 => u32::MAX as i128,
            Self::U64 => u64::MAX as i128,
            Self::U128 => i128::MAX, // u128::MAX doesn't fit i128; pin to i128::MAX.
            Self::USize => u64::MAX as i128, // FR-011: usize ≡ u64.
            Self::ISize => i64::MAX as i128, // FR-011: isize ≡ i64.
        }
    }

    /// `true` iff `v` is in this type's representable range.
    pub fn contains(self, v: i128) -> bool {
        v >= self.min_value() && v <= self.max_value()
    }

    /// `true` for signed-integer kinds (i*, isize). `false` for unsigned.
    pub fn is_signed(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::I128 | Self::ISize
        )
    }

    /// Rust type-name verbatim (`"u8"`, `"i64"`, `"usize"`, …).
    pub fn name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::ISize => "isize",
            Self::USize => "usize",
        }
    }
}

/// **M03.2**: float-kind discriminator. Used by `Ty::Float` and `Value::Float`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)] // Variant names are self-documenting (`F32`, `F64`).
pub enum FloatKind {
    F32, F64,
}

#[cfg(test)]
mod intkind_tests {
    use super::*;

    #[test]
    fn u8_range() {
        assert_eq!(IntKind::U8.min_value(), 0);
        assert_eq!(IntKind::U8.max_value(), 255);
        assert!(IntKind::U8.contains(0));
        assert!(IntKind::U8.contains(255));
        assert!(!IntKind::U8.contains(256));
        assert!(!IntKind::U8.contains(-1));
    }

    #[test]
    fn i8_range() {
        assert_eq!(IntKind::I8.min_value(), -128);
        assert_eq!(IntKind::I8.max_value(), 127);
        assert!(IntKind::I8.contains(-128));
        assert!(IntKind::I8.contains(127));
        assert!(!IntKind::I8.contains(128));
        assert!(!IntKind::I8.contains(-129));
    }

    #[test]
    fn usize_matches_u64() {
        assert_eq!(IntKind::USize.min_value(), IntKind::U64.min_value());
        assert_eq!(IntKind::USize.max_value(), IntKind::U64.max_value());
    }

    #[test]
    fn isize_matches_i64() {
        assert_eq!(IntKind::ISize.min_value(), IntKind::I64.min_value());
        assert_eq!(IntKind::ISize.max_value(), IntKind::I64.max_value());
    }

    #[test]
    fn is_signed_exhaustive() {
        for k in [IntKind::I8, IntKind::I16, IntKind::I32, IntKind::I64, IntKind::I128, IntKind::ISize] {
            assert!(k.is_signed(), "{} should be signed", k.name());
        }
        for k in [IntKind::U8, IntKind::U16, IntKind::U32, IntKind::U64, IntKind::U128, IntKind::USize] {
            assert!(!k.is_signed(), "{} should be unsigned", k.name());
        }
    }

    #[test]
    fn names_match_rust() {
        assert_eq!(IntKind::U8.name(), "u8");
        assert_eq!(IntKind::I64.name(), "i64");
        assert_eq!(IntKind::USize.name(), "usize");
        assert_eq!(FloatKind::F32.name(), "f32");
        assert_eq!(FloatKind::F64.name(), "f64");
    }
}

impl FloatKind {
    /// Rust type-name verbatim (`"f32"` or `"f64"`).
    pub fn name(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }
}

/// L1+L2 value types. **M03.2**: restructured into nested kind enums.
/// **M06**: adds `Ref { inner, mutable }`. `Box<Ty>` makes the recursive
/// `Ty::Ref` shape work, dropping the `Copy` derive — methods now take
/// `&self`. Function signatures live in [`FnSig`], not here, because
/// functions are not first-class values in L1.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Ty {
    /// Signed or unsigned integer. Width is carried by [`IntKind`].
    Int(IntKind),
    /// Floating-point. Width is carried by [`FloatKind`].
    Float(FloatKind),
    /// Boolean.
    Bool,
    /// Unit `()`.
    Unit,
    /// **M06**: reference type. `&T` if `mutable == false`, `&mut T` otherwise.
    Ref {
        /// Pointed-to type.
        inner: Box<Ty>,
        /// `true` for `&mut`, `false` for `&`.
        mutable: bool,
    },
    /// **M07**: heap-owning `Box<T>`. Non-Copy.
    Box(Box<Ty>),
    /// **M07**: heap-owning `Vec<T>`. Non-Copy.
    Vec(Box<Ty>),
    /// **M07**: heap-owning UTF-8 byte sequence. Non-Copy.
    String,
    /// **M07.1**: slice type `&[T]`. Always shared (immutable) in M07.1.
    /// The leading `&` is absorbed into this variant — `Ty::Slice(T)` IS
    /// the `&[T]` type, matching Rust's "[T] only appears behind a
    /// reference" rule. Carries the element type.
    Slice(Box<Ty>),
    /// **M07.2**: `&str` — semantically equivalent to
    /// `Ty::Slice(Box::new(Ty::Int(IntKind::U8)))`. Kept as a distinct
    /// sugar variant so the rendered type reads `"&str"` (not `"&[u8]"`),
    /// matching what Rust developers see. Method dispatch + borrow
    /// tracking treat `Ty::Str` interchangeably with the slice form.
    Str,
    /// **M07.3**: array type `[T; N]`. Stack-allocated, fixed size known
    /// at compile time. Distinct from `Ty::Vec(T)` (heap-allocated,
    /// runtime size) and `Ty::Slice(T)` (size-erased borrow). Copy iff
    /// `T: Copy` — M07.3 restricts elements to primitives so always Copy.
    Array(Box<Ty>, u64),
    /// **M07.4**: nominal struct type. Equality is by `name` only (the
    /// `fields` are carried for convenience so callers can read the
    /// schema without a registry round-trip; they're redundant for
    /// identity). M07.4 fields are restricted to primitives so the
    /// struct is always Copy.
    Struct {
        /// Type name (e.g. `"Point"`).
        name: String,
        /// Fields in declaration order. Mirrors `StructDecl.fields`.
        fields: Vec<(String, Ty)>,
    },
}

impl Ty {
    /// Render this type as a user-facing string (`"u8"`, `"f64"`, `"&i32"`,
    /// `"&mut bool"`, `"bool"`, `"()"`). Allocates because of `Ref`'s
    /// recursive inner.
    pub fn name(&self) -> String {
        match self {
            Self::Int(k) => k.name().to_owned(),
            Self::Float(k) => k.name().to_owned(),
            Self::Bool => "bool".to_owned(),
            Self::Unit => "()".to_owned(),
            Self::Ref { inner, mutable } => {
                if *mutable {
                    format!("&mut {}", inner.name())
                } else {
                    format!("&{}", inner.name())
                }
            }
            Self::Box(inner) => format!("Box<{}>", inner.name()),
            Self::Vec(inner) => format!("Vec<{}>", inner.name()),
            Self::String => "String".to_owned(),
            // M07.1: slice. Leading `&` is part of the type name; always shared in M07.1.
            Self::Slice(inner) => format!("&[{}]", inner.name()),
            // M07.2: &str sugar — rendered as `"&str"` not `"&[u8]"`.
            Self::Str => "&str".to_owned(),
            // M07.3: array — `[T; N]`.
            Self::Array(inner, size) => format!("[{}; {}]", inner.name(), size),
            // M07.4: struct — bare type name (matches Rust's nominal typing).
            Self::Struct { name, .. } => name.clone(),
        }
    }

    /// Whether values of this type are `Copy` (no destructor; bytes physically
    /// persist on the stack until storage is reused). M06: `&T` is Copy;
    /// `&mut T` is not (matches Rust). **M07**: Box, Vec, String are
    /// non-Copy (heap-owning types with destructors). Exhaustive match.
    pub fn is_copy(&self) -> bool {
        match self {
            Self::Int(_) | Self::Float(_) | Self::Bool | Self::Unit => true,
            Self::Ref { mutable: false, .. } => true,
            Self::Ref { mutable: true, .. } => false,
            Self::Box(_) | Self::Vec(_) | Self::String => false,
            // M07.1: slices are non-Copy (they carry a borrow_id; cloning would
            // duplicate the borrow registration).
            // M07.2: &str follows the same rule (it's a Slice<u8> in disguise).
            Self::Slice(_) | Self::Str => false,
            // M07.3: arrays are Copy iff their element type is Copy. M07.3
            // restricts elements to primitives (all Copy), so always true.
            Self::Array(inner, _) => inner.is_copy(),
            // M07.4: structs are Copy iff all fields are Copy. M07.4
            // restricts fields to primitives so always true; future
            // milestones with non-Copy fields will produce false here.
            Self::Struct { fields, .. } => fields.iter().all(|(_, t)| t.is_copy()),
        }
    }
}

/// Function signature: parameter types and return type.
#[derive(Debug, Clone, PartialEq)]
pub struct FnSig {
    /// Parameter types in declaration order.
    pub params: Vec<Ty>,
    /// Return type. `Ty::Unit` if the function has no `-> T` annotation.
    pub ret: Ty,
}

/// Type information attached to a binding.
#[derive(Debug, Clone, PartialEq)]
pub enum BindingType {
    /// Binding holds a value of this type (let / param).
    Var(Ty),
    /// Binding is a function with this signature.
    Fn(FnSig),
}

/// Output of [`typeck`]. Two side tables — one keyed by expression span, one
/// keyed by binding id.
#[derive(Debug, Clone, Default)]
pub struct TypeMap {
    /// Maps each value-producing `Expr` (by span) to its inferred [`Ty`].
    /// The callee Ident of a `Call` is intentionally absent (it's a function
    /// reference, not a value). Iteration order is tree-walk pre-order
    /// (research.md R-002).
    pub expr_types: IndexMap<Span, Ty>,
    /// Maps each `BindingId` to its [`BindingType`].
    pub binding_types: IndexMap<BindingId, BindingType>,
}

/// Type-check a resolved program.
///
/// On success, returns a `TypeMap` with `expr_types` covering every
/// value-producing expression. On failure, returns a single `ParseError`.
pub fn typeck(program: &ast::Program, resolution: &Resolution) -> Result<TypeMap, ParseError> {
    let mut t = Typechecker::new(program, resolution);

    // Phase 1: collect struct schemas + impl-block signatures into the
    // typeck-side registries (M07.4) AND compute FnSig for every top-level
    // free fn item, seeding binding_types.
    // **M07.4**: structs first so impl blocks can reference them.
    for item in &program.items {
        if let ast::Item::Struct(decl) = item {
            t.register_struct(decl)?;
        }
    }
    for item in &program.items {
        match item {
            ast::Item::Fn(decl) => {
                let sig = t.build_fn_sig(decl)?;
                let id = t
                    .lookup_binding(|d| matches!(d.kind, BindingKind::Fn) && d.name == decl.name)
                    .expect("fn binding present after resolve");
                t.types.binding_types.insert(id, BindingType::Fn(sig));
            }
            // Structs already processed above.
            ast::Item::Struct(_) => {}
            // M07.4 impl blocks register methods + assoc fns.
            ast::Item::Impl(block) => t.register_impl(block)?,
        }
    }

    // Phase 2: typecheck each fn body. Free fns go through the regular
    // typecheck_fn path. Impl-block fns (methods + assoc fns) use the
    // dedicated typecheck_impl_fn helper which handles the self-receiver
    // placeholder substitution.
    for item in &program.items {
        match item {
            ast::Item::Fn(decl) => t.typecheck_fn(decl)?,
            ast::Item::Struct(_) => {}
            ast::Item::Impl(block) => {
                for fn_decl in &block.items {
                    t.typecheck_impl_fn(&block.ty_name, fn_decl)?;
                }
            }
        }
    }

    Ok(t.types)
}

/// **M06**: borrow-checker module. Tracks active borrows per binding and
/// enforces Rust's aliasing rules statically (scope-level lifetimes).
#[allow(unreachable_pub)] // private mod; pub items are inner-visible from typeck.
mod borrow_tracker {
    use crate::parse::span::Span;
    use crate::resolve::BindingId;
    use indexmap::IndexMap;

    /// What kind of borrow is active.
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub enum BorrowKind {
        Shared,
        Mut,
    }

    /// One active borrow recorded against a binding.
    #[derive(Clone, Debug)]
    pub struct ActiveBorrow {
        pub kind: BorrowKind,
        pub scope_depth: u32,
        pub borrow_span: Span,
    }

    /// Result of a failed `try_take_*` — carries the conflicting existing borrow.
    #[derive(Clone, Debug)]
    pub struct AliasConflict {
        pub existing_kind: BorrowKind,
        #[allow(dead_code)] // reserved for richer error messages
        pub existing_span: Span,
    }

    #[derive(Default)]
    pub struct BorrowTracker {
        active: IndexMap<BindingId, Vec<ActiveBorrow>>,
    }

    impl BorrowTracker {
        pub fn new() -> Self {
            Self::default()
        }

        /// Take a shared borrow. Fails if any active borrow of `b` is `Mut`.
        pub fn try_take_shared(
            &mut self,
            b: BindingId,
            depth: u32,
            span: Span,
        ) -> Result<(), AliasConflict> {
            let stack = self.active.entry(b).or_default();
            if let Some(existing) = stack.iter().find(|a| a.kind == BorrowKind::Mut) {
                return Err(AliasConflict {
                    existing_kind: existing.kind,
                    existing_span: existing.borrow_span,
                });
            }
            stack.push(ActiveBorrow {
                kind: BorrowKind::Shared,
                scope_depth: depth,
                borrow_span: span,
            });
            Ok(())
        }

        /// Take a mutable borrow. Fails if any active borrow of `b` exists.
        pub fn try_take_mut(
            &mut self,
            b: BindingId,
            depth: u32,
            span: Span,
        ) -> Result<(), AliasConflict> {
            let stack = self.active.entry(b).or_default();
            if let Some(existing) = stack.last() {
                return Err(AliasConflict {
                    existing_kind: existing.kind,
                    existing_span: existing.borrow_span,
                });
            }
            stack.push(ActiveBorrow {
                kind: BorrowKind::Mut,
                scope_depth: depth,
                borrow_span: span,
            });
            Ok(())
        }

        /// Drop all borrows recorded at or deeper than `leaving_depth`.
        pub fn pop_scope(&mut self, leaving_depth: u32) {
            for stack in self.active.values_mut() {
                stack.retain(|a| a.scope_depth < leaving_depth);
            }
        }

        /// **M06.1**: whether `b` has any active borrows. Used by typeck to
        /// reject direct assignment to a currently-borrowed binding.
        pub fn is_borrowed(&self, b: BindingId) -> bool {
            self.active.get(&b).map(|v| !v.is_empty()).unwrap_or(false)
        }
    }
}

/// **M07.4**: per-struct schema collected during phase 1. Maps struct name →
/// declaration-ordered field list. Phase 2 consults this to typecheck struct
/// literals, field accesses, and field borrows.
#[derive(Default)]
struct StructRegistry {
    schemas: IndexMap<String, Vec<(String, Ty)>>,
    /// Spans of the declaring `Item::Struct` for diagnostic anchoring on
    /// duplicate-definition errors.
    decl_spans: IndexMap<String, Span>,
}

/// **M07.4**: collected method + associated-function signatures from `impl`
/// blocks. Built during phase 1 so phase 2 dispatch lookups have full
/// visibility regardless of source order.
#[derive(Default)]
struct ImplRegistry {
    /// `(struct_name, method_name)` → method signature (with self-receiver
    /// info stripped from `params` — recorded on `ParamKind`).
    methods: IndexMap<(String, String), FnSig>,
    /// `vec!["Struct", "name"]` → associated-fn signature.
    assoc_fns: IndexMap<Vec<String>, FnSig>,
    /// Spans for diagnostics.
    method_spans: IndexMap<(String, String), Span>,
    assoc_fn_spans: IndexMap<Vec<String>, Span>,
    /// Tracks struct names that already have one impl block so duplicates
    /// can be rejected per the M07.4 "one impl block per type" rule.
    impl_block_spans: IndexMap<String, Span>,
}

struct Typechecker<'a> {
    resolution: &'a Resolution,
    types: TypeMap,
    /// Expected return type of the function currently being checked.
    current_fn_ret: Option<Ty>,
    /// **M06**: active borrows for static aliasing-rule enforcement.
    borrow_tracker: borrow_tracker::BorrowTracker,
    /// **M06**: current scope depth (incremented on block enter, decremented on exit).
    scope_depth: u32,
    /// **M07.4**: struct schemas collected in phase 1.
    structs: StructRegistry,
    /// **M07.4**: method + assoc-fn registry collected in phase 1.
    impls: ImplRegistry,
}

impl<'a> Typechecker<'a> {
    fn new(_program: &'a ast::Program, resolution: &'a Resolution) -> Self {
        Self {
            resolution,
            types: TypeMap::default(),
            current_fn_ret: None,
            borrow_tracker: borrow_tracker::BorrowTracker::new(),
            scope_depth: 0,
            structs: StructRegistry::default(),
            impls: ImplRegistry::default(),
        }
    }

    /// **M07.4**: phase-1 — register a struct's schema. Rejects duplicate
    /// struct names and non-primitive field types.
    fn register_struct(&mut self, decl: &ast::StructDecl) -> Result<(), ParseError> {
        if let Some(prev) = self.structs.decl_spans.get(&decl.name) {
            let prev_line = prev.start; // raw byte offset; good enough for the message
            return Err(ParseError {
                message: format!(
                    "struct `{}` already defined (previous definition at byte {prev_line})",
                    decl.name
                ),
                span: decl.span,
            });
        }
        let mut fields: Vec<(String, Ty)> = Vec::with_capacity(decl.fields.len());
        for field in &decl.fields {
            let ty = ty_from_ast(&field.ty)?;
            // M07.4 restricts fields to primitive types (Int, Float, Bool,
            // Unit). Non-Copy / composite types are out of scope.
            let primitive = matches!(
                ty,
                Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Unit
            );
            if !primitive {
                return Err(ParseError {
                    message: format!(
                        "field `{}` of struct `{}` has type `{}`; M07.4 fields must be primitive types (i32, bool, etc.)",
                        field.name,
                        decl.name,
                        ty.name()
                    ),
                    span: field.span,
                });
            }
            fields.push((field.name.clone(), ty));
        }
        self.structs.schemas.insert(decl.name.clone(), fields);
        self.structs.decl_spans.insert(decl.name.clone(), decl.span);
        Ok(())
    }

    /// **M07.4**: struct-aware `ty_from_ast` wrapper. Used by phase-1 fn
    /// signature collection (free + impl-block) so `-> Point` and
    /// param types like `p: Point` resolve to `Ty::Struct(...)` instead
    /// of "unknown type `Point`". Falls back to the standard `ty_from_ast`
    /// for everything else.
    fn ty_from_ast_resolving_structs(&self, t: &ast::Type) -> Result<Ty, ParseError> {
        if let ast::Type::Path { segments, .. } = t {
            if segments.len() == 1 {
                if let Some(fields) = self.structs.schemas.get(&segments[0]) {
                    return Ok(Ty::Struct {
                        name: segments[0].clone(),
                        fields: fields.clone(),
                    });
                }
            }
        }
        ty_from_ast(t)
    }

    /// **M07.4**: phase-1 — register an impl block's methods + assoc fns
    /// into the dispatch tables. Verifies the type exists, rejects a second
    /// impl block for the same type, and rejects duplicate item names.
    fn register_impl(&mut self, block: &ast::ImplBlock) -> Result<(), ParseError> {
        if !self.structs.schemas.contains_key(&block.ty_name) {
            return Err(ParseError {
                message: format!(
                    "impl block references unknown type `{}` (M07.4 supports inherent impls on user-defined structs only — not on built-ins like Vec or String)",
                    block.ty_name
                ),
                span: block.span,
            });
        }
        if let Some(prev_span) = self.impls.impl_block_spans.get(&block.ty_name) {
            let _ = prev_span;
            return Err(ParseError {
                message: format!(
                    "M07.4 supports only one impl block per type; merge into a single block (existing impl block for `{}` already declared)",
                    block.ty_name
                ),
                span: block.span,
            });
        }
        self.impls.impl_block_spans.insert(block.ty_name.clone(), block.span);
        for fn_decl in &block.items {
            self.register_impl_fn(&block.ty_name, fn_decl)?;
        }
        Ok(())
    }

    /// **M07.4**: register one fn item inside an impl block. The first
    /// param's `kind` distinguishes method (self-receiver) from associated
    /// fn (no self). For methods, the self-receiver's placeholder type is
    /// swapped for the real `Ty::Struct(_)` or `Ty::Ref { Ty::Struct(_), .. }`.
    fn register_impl_fn(
        &mut self,
        struct_name: &str,
        decl: &ast::FnDecl,
    ) -> Result<(), ParseError> {
        // Resolve the struct's Ty for self-receiver substitution.
        let struct_ty = Ty::Struct {
            name: struct_name.to_owned(),
            fields: self.structs.schemas[struct_name].clone(),
        };
        // Build the signature, treating the self-receiver (if present) as
        // ALREADY consumed — `params` here is the explicit-only param list.
        let mut explicit_params: Vec<Ty> = Vec::new();
        let mut self_kind: Option<ast::ParamKind> = None;
        for (i, param) in decl.params.iter().enumerate() {
            if i == 0 && !matches!(param.kind, ast::ParamKind::Normal) {
                self_kind = Some(param.kind);
                continue;
            }
            if !matches!(param.kind, ast::ParamKind::Normal) {
                return Err(ParseError {
                    message: "`self` parameter must be the first parameter".into(),
                    span: param.span,
                });
            }
            explicit_params.push(self.ty_from_ast_resolving_structs(&param.ty)?);
        }
        let ret = match &decl.return_ty {
            Some(t) => self.ty_from_ast_resolving_structs(t)?,
            None => Ty::Unit,
        };
        let sig = FnSig { params: explicit_params, ret };
        match self_kind {
            Some(_) => {
                // Method. The self-receiver type is implicit (struct_ty for
                // SelfOwned, &struct_ty / &mut struct_ty for SelfShared/SelfMut)
                // and recorded separately via Param.kind on the FnDecl AST node.
                let _ = struct_ty;
                let key = (struct_name.to_owned(), decl.name.clone());
                if self.impls.methods.contains_key(&key) {
                    return Err(ParseError {
                        message: format!(
                            "method `{}` already defined on `{}`",
                            decl.name, struct_name
                        ),
                        span: decl.span,
                    });
                }
                self.impls.method_spans.insert(key.clone(), decl.span);
                self.impls.methods.insert(key, sig);
            }
            None => {
                let key = vec![struct_name.to_owned(), decl.name.clone()];
                if self.impls.assoc_fns.contains_key(&key) {
                    return Err(ParseError {
                        message: format!(
                            "associated fn `{}` already defined on `{}`",
                            decl.name, struct_name
                        ),
                        span: decl.span,
                    });
                }
                self.impls.assoc_fn_spans.insert(key.clone(), decl.span);
                self.impls.assoc_fns.insert(key, sig);
            }
        }
        Ok(())
    }

    fn lookup_binding(&self, mut pred: impl FnMut(&crate::resolve::BindingDecl) -> bool) -> Option<BindingId> {
        self.resolution
            .bindings
            .iter()
            .find_map(|(id, decl)| if pred(decl) { Some(*id) } else { None })
    }

    fn build_fn_sig(&self, decl: &ast::FnDecl) -> Result<FnSig, ParseError> {
        let mut params = Vec::with_capacity(decl.params.len());
        for param in &decl.params {
            params.push(self.ty_from_ast_resolving_structs(&param.ty)?);
        }
        let ret = match &decl.return_ty {
            Some(t) => self.ty_from_ast_resolving_structs(t)?,
            None => Ty::Unit,
        };
        Ok(FnSig { params, ret })
    }

    fn typecheck_fn(&mut self, decl: &ast::FnDecl) -> Result<(), ParseError> {
        let fn_id = self
            .lookup_binding(|d| matches!(d.kind, BindingKind::Fn) && d.name == decl.name)
            .expect("fn binding present");
        let sig = match self.types.binding_types.get(&fn_id) {
            Some(BindingType::Fn(s)) => s.clone(),
            _ => panic!("fn sig must be set in Phase 1"),
        };
        for (param, param_ty) in decl.params.iter().zip(sig.params.iter()) {
            let pid = self
                .lookup_binding(|d| matches!(d.kind, BindingKind::Param) && d.name_span == param.span)
                .expect("param binding present");
            self.types
                .binding_types
                .insert(pid, BindingType::Var(param_ty.clone()));
        }
        let prev = self.current_fn_ret.replace(sig.ret.clone());
        let body_ty = self.typecheck_block(&decl.body)?;
        if body_ty != sig.ret {
            return Err(ParseError {
                message: format!(
                    "function returns `{}`, but body has type `{}`",
                    sig.ret.name(),
                    body_ty.name()
                ),
                span: decl
                    .body
                    .tail
                    .as_ref()
                    .map(|t| t.span())
                    .unwrap_or(decl.body.span),
            });
        }
        self.current_fn_ret = prev;
        Ok(())
    }

    /// **M07.4**: typecheck an impl-block fn body. Mirrors `typecheck_fn`
    /// but:
    ///   - Substitutes the self-receiver's placeholder type with the
    ///     enclosing impl block's real `Ty::Struct` (or borrow thereof).
    ///   - Reads the sig from `impls.methods` / `impls.assoc_fns` rather
    ///     than `binding_types` (impl-block fns have no top-level Fn id).
    fn typecheck_impl_fn(
        &mut self,
        struct_name: &str,
        decl: &ast::FnDecl,
    ) -> Result<(), ParseError> {
        let struct_ty = Ty::Struct {
            name: struct_name.to_owned(),
            fields: self.structs.schemas[struct_name].clone(),
        };
        // Look up the explicit-only sig from the registries.
        let key_method = (struct_name.to_owned(), decl.name.clone());
        let key_assoc = vec![struct_name.to_owned(), decl.name.clone()];
        let sig = self
            .impls
            .methods
            .get(&key_method)
            .or_else(|| self.impls.assoc_fns.get(&key_assoc))
            .cloned()
            .expect("impl fn registered in phase 1");
        // Bind every param's type. For self-receivers, use the substituted
        // type; for normal params, zip against sig.params.
        let mut explicit_iter = sig.params.iter();
        for (i, param) in decl.params.iter().enumerate() {
            let bind_ty = if i == 0 {
                match param.kind {
                    ast::ParamKind::SelfOwned => struct_ty.clone(),
                    ast::ParamKind::SelfShared => Ty::Ref {
                        inner: Box::new(struct_ty.clone()),
                        mutable: false,
                    },
                    ast::ParamKind::SelfMut => Ty::Ref {
                        inner: Box::new(struct_ty.clone()),
                        mutable: true,
                    },
                    ast::ParamKind::Normal => explicit_iter.next().expect("normal sig").clone(),
                }
            } else {
                explicit_iter.next().expect("normal sig").clone()
            };
            let pid = self
                .lookup_binding(|d| matches!(d.kind, BindingKind::Param) && d.name_span == param.span)
                .expect("param binding present");
            self.types
                .binding_types
                .insert(pid, BindingType::Var(bind_ty));
        }
        let prev = self.current_fn_ret.replace(sig.ret.clone());
        let body_ty = self.typecheck_block(&decl.body)?;
        if body_ty != sig.ret {
            return Err(ParseError {
                message: format!(
                    "function returns `{}`, but body has type `{}`",
                    sig.ret.name(),
                    body_ty.name()
                ),
                span: decl
                    .body
                    .tail
                    .as_ref()
                    .map(|t| t.span())
                    .unwrap_or(decl.body.span),
            });
        }
        self.current_fn_ret = prev;
        Ok(())
    }

    fn typecheck_block(&mut self, block: &ast::Block) -> Result<Ty, ParseError> {
        // M06: scope-level lifetime tracking. Increment depth on entry, drop
        // borrows recorded at this depth on exit.
        self.scope_depth += 1;
        let result = (|| -> Result<Ty, ParseError> {
            for stmt in &block.stmts {
                self.typecheck_stmt(stmt)?;
            }
            if let Some(tail) = &block.tail {
                self.typecheck_expr(tail)
            } else {
                Ok(Ty::Unit)
            }
        })();
        self.borrow_tracker.pop_scope(self.scope_depth);
        self.scope_depth -= 1;
        result
    }

    fn typecheck_stmt(&mut self, stmt: &ast::Stmt) -> Result<(), ParseError> {
        match stmt {
            ast::Stmt::Let(let_stmt) => {
                let init_ty = self.typecheck_expr(&let_stmt.init)?;
                let bind_ty = match &let_stmt.ty {
                    Some(annot) => {
                        let annot_ty = self.ty_from_ast_resolving_structs(annot)?;
                        // M03.2: attempt to coerce a literal init to the annotated
                        // type before checking equality. Allows `let x: u8 = 5;`.
                        let init_ty = self
                            .try_coerce_to(&let_stmt.init, init_ty, annot_ty.clone())
                            .unwrap_or_else(|| self
                                .types
                                .expr_types
                                .get(&let_stmt.init.span())
                                .cloned()
                                .unwrap_or(annot_ty.clone()));
                        if annot_ty != init_ty {
                            return Err(ParseError {
                                message: format!(
                                    "expected `{}`, found `{}`",
                                    annot_ty.name(),
                                    init_ty.name()
                                ),
                                span: let_stmt.init.span(),
                            });
                        }
                        annot_ty
                    }
                    None => init_ty,
                };
                let id = self
                    .lookup_binding(|d| {
                        matches!(d.kind, BindingKind::Let { .. }) && d.name_span == let_stmt.span
                    })
                    .expect("let binding present");
                self.types.binding_types.insert(id, BindingType::Var(bind_ty));
            }
            ast::Stmt::Expr(expr) => {
                self.typecheck_expr(expr)?;
            }
            // **M06.1**: assignment statement `lhs = rhs;`. Handles both
            // direct assignment (US1: `Expr::Ident(x)` lhs) and through-ref
            // assignment (US3: `Expr::Deref(Expr::Ident(r))` lhs).
            ast::Stmt::Assign { lhs, rhs, span } => {
                self.typecheck_assign(lhs, rhs, *span)?;
            }
        }
        Ok(())
    }

    /// **M06.1**: typecheck an assignment statement. The lhs must be a place
    /// expression (`Expr::Ident(x)` with `x: let mut`, OR
    /// `Expr::Deref(Expr::Ident(r))` with `r: &mut T`). The rhs must
    /// typecheck to the same type (with M03.2 literal coercion).
    fn typecheck_assign(
        &mut self,
        lhs: &ast::Expr,
        rhs: &ast::Expr,
        span: Span,
    ) -> Result<(), ParseError> {
        // Determine the lhs's expected type and the binding being mutated.
        let lhs_ty = match lhs {
            ast::Expr::Ident(name, ident_span) => {
                // Direct assignment to a `let mut` binding.
                let binding_id = *self
                    .resolution
                    .uses
                    .get(ident_span)
                    .expect("ident resolved");
                let decl = &self.resolution.bindings[&binding_id];
                let is_mut_let = matches!(decl.kind, BindingKind::Let { mutable: true, .. });
                if !is_mut_let {
                    return Err(ParseError {
                        message: format!("cannot assign to immutable variable `{name}`"),
                        span: *ident_span,
                    });
                }
                // M06: cannot assign to a borrowed binding.
                if self.borrow_tracker.is_borrowed(binding_id) {
                    return Err(ParseError {
                        message: format!(
                            "cannot assign to `{name}` because it is borrowed"
                        ),
                        span: *ident_span,
                    });
                }
                // Look up x's current type.
                match self.types.binding_types.get(&binding_id) {
                    Some(BindingType::Var(t)) => t.clone(),
                    _ => panic!("typeck saw an unbound or non-var ident at assign lhs"),
                }
            }
            ast::Expr::Deref { inner, span: deref_span } => {
                // Through-ref assignment: inner must be Ident, its type must
                // be `&mut T`. No borrow-tracker check (R-008): the `&mut`
                // itself is what permits the write.
                let inner_ident = match inner.as_ref() {
                    ast::Expr::Ident(_, _) => inner.as_ref(),
                    _ => {
                        return Err(ParseError {
                            message: "left side of assignment must be a place expression".into(),
                            span: lhs.span(),
                        });
                    }
                };
                let inner_ty = self.typecheck_expr(inner_ident)?;
                match inner_ty {
                    Ty::Ref { inner: target, mutable: true } => {
                        // Also record the deref's own type in expr_types so
                        // future consumers don't panic.
                        self.types.expr_types.insert(*deref_span, (*target).clone());
                        *target
                    }
                    Ty::Ref { mutable: false, .. } => {
                        return Err(ParseError {
                            message: "cannot assign through `&T`; need `&mut T`".into(),
                            span: *deref_span,
                        });
                    }
                    other => {
                        return Err(ParseError {
                            message: format!(
                                "cannot dereference value of type `{}`; expected a reference",
                                other.name()
                            ),
                            span: inner.span(),
                        });
                    }
                }
            }
            // **M07.4**: field assignment `p.x = rhs;`. Place check: the
            // receiver must be an `Expr::Ident` resolving to a mutable
            // struct binding; the field must exist; the rhs must coerce to
            // the field's type. Eval will read the slot's current
            // Value::Struct, mutate the named field, emit a SlotWrite with
            // the updated struct.
            ast::Expr::FieldAccess { receiver, name, span: fa_span } => {
                let recv_ident_span = match receiver.as_ref() {
                    ast::Expr::Ident(_, sp) => *sp,
                    _ => {
                        return Err(ParseError {
                            message: "M07.4 field assignment requires a direct binding receiver (`p.x = v;`); chained field access (`p.x.y = v;`) is out of scope".into(),
                            span: receiver.span(),
                        });
                    }
                };
                let recv_binding = *self
                    .resolution
                    .uses
                    .get(&recv_ident_span)
                    .expect("ident resolved");
                let recv_decl = &self.resolution.bindings[&recv_binding];
                let recv_name = recv_decl.name.clone();
                let is_mut_let = matches!(
                    recv_decl.kind,
                    BindingKind::Let { mutable: true, .. },
                );
                if !is_mut_let {
                    return Err(ParseError {
                        message: format!(
                            "cannot assign to field of immutable variable `{recv_name}`"
                        ),
                        span: recv_ident_span,
                    });
                }
                if self.borrow_tracker.is_borrowed(recv_binding) {
                    return Err(ParseError {
                        message: format!(
                            "cannot assign to field of `{recv_name}` because it is borrowed"
                        ),
                        span: recv_ident_span,
                    });
                }
                let recv_ty = match self.types.binding_types.get(&recv_binding) {
                    Some(BindingType::Var(t)) => t.clone(),
                    _ => panic!("typeck saw non-var ident at field-assign lhs"),
                };
                let schema = match &recv_ty {
                    Ty::Struct { fields, .. } => fields.clone(),
                    other => {
                        return Err(ParseError {
                            message: format!(
                                "field assignment requires a struct receiver, found `{}`",
                                other.name()
                            ),
                            span: receiver.span(),
                        });
                    }
                };
                let field_ty = match schema.iter().find(|(n, _)| n == name) {
                    Some((_, t)) => t.clone(),
                    None => {
                        let struct_name = match &recv_ty {
                            Ty::Struct { name, .. } => name.clone(),
                            _ => "<unknown>".to_owned(),
                        };
                        return Err(ParseError {
                            message: format!(
                                "no field `{name}` on struct `{struct_name}`"
                            ),
                            span: *fa_span,
                        });
                    }
                };
                // Record the lhs FieldAccess's type so eval/snapshots see it.
                self.types.expr_types.insert(*fa_span, field_ty.clone());
                // Record the receiver's type too (eval may consult).
                self.types.expr_types.insert(recv_ident_span, recv_ty);
                field_ty
            }
            _ => {
                return Err(ParseError {
                    message: "left side of assignment must be a place expression".into(),
                    span: lhs.span(),
                });
            }
        };
        // Typecheck rhs and coerce to lhs's type if it's a literal.
        let rhs_ty = self.typecheck_expr(rhs)?;
        let rhs_ty = self
            .try_coerce_to(rhs, rhs_ty.clone(), lhs_ty.clone())
            .unwrap_or(rhs_ty);
        if rhs_ty != lhs_ty {
            return Err(ParseError {
                message: format!(
                    "expected `{}`, found `{}`",
                    lhs_ty.name(),
                    rhs_ty.name()
                ),
                span: rhs.span(),
            });
        }
        // Record the statement's "expression type" implicitly as Unit; not
        // strictly necessary since Stmt doesn't expose a type to callers.
        let _ = span;
        Ok(())
    }

    /// Typecheck an expression and record its type in `expr_types`. Returns the type.
    fn typecheck_expr(&mut self, expr: &ast::Expr) -> Result<Ty, ParseError> {
        let ty = self.typecheck_expr_inner(expr)?;
        self.types.expr_types.insert(expr.span(), ty.clone());
        Ok(ty)
    }

    fn typecheck_expr_inner(&mut self, expr: &ast::Expr) -> Result<Ty, ParseError> {
        match expr {
            // **M03.2**: a literal with an explicit suffix uses that kind directly;
            // no coercion needed. Without a suffix, default to I32 / F64.
            ast::Expr::LitInt(_, suffix, _) => Ok(Ty::Int(suffix.unwrap_or(IntKind::I32))),
            ast::Expr::LitFloat(_, suffix, _) => Ok(Ty::Float(suffix.unwrap_or(FloatKind::F64))),
            ast::Expr::LitBool(_, _) => Ok(Ty::Bool),
            ast::Expr::Ident(_, span) => {
                let id = *self
                    .resolution
                    .uses
                    .get(span)
                    .expect("ident use resolved during resolve()");
                match self.types.binding_types.get(&id) {
                    Some(BindingType::Var(ty)) => Ok(ty.clone()),
                    Some(BindingType::Fn(_)) => {
                        let name = self.resolution.bindings[&id].name.clone();
                        Err(ParseError {
                            message: format!(
                                "`{name}` is a function; functions are not first-class values in L1"
                            ),
                            span: *span,
                        })
                    }
                    None => panic!("binding {id:?} has no type"),
                }
            }
            ast::Expr::Unary { op, expr: inner, span } => {
                let inner_ty = self.typecheck_expr(inner)?;
                // M03.2: unary `-` works on any signed-integer kind or float.
                // Unsigned types reject (matches Rust's missing Neg impl).
                if let ast::UnOp::Neg = op {
                    match inner_ty {
                        Ty::Int(k) if k.is_signed() => return Ok(inner_ty),
                        Ty::Float(_) => return Ok(inner_ty),
                        Ty::Int(k) => {
                            return Err(ParseError {
                                message: format!(
                                    "cannot apply unary `-` to `{}` (unsigned types don't impl Neg)",
                                    k.name()
                                ),
                                span: *span,
                            });
                        }
                        _ => {
                            return Err(ParseError {
                                message: format!(
                                    "unary operator `-` requires a numeric operand, found `{}`",
                                    inner_ty.name()
                                ),
                                span: *span,
                            });
                        }
                    }
                }
                let expected = match op {
                    ast::UnOp::Neg => unreachable!("handled above"),
                    ast::UnOp::Not => Ty::Bool,
                };
                if inner_ty != expected {
                    return Err(ParseError {
                        message: format!(
                            "unary operator `{}` requires `{}`, found `{}`",
                            unop_str(*op),
                            expected.name(),
                            inner_ty.name()
                        ),
                        span: *span,
                    });
                }
                Ok(expected)
            }
            ast::Expr::Binary { op, lhs, rhs, span } => {
                self.typecheck_binary(*op, lhs, rhs, *span)
            }
            ast::Expr::Call { callee, args, span } => self.typecheck_call(callee, args, *span),
            ast::Expr::Paren { inner, .. } => self.typecheck_expr(inner),
            ast::Expr::Block(block) => self.typecheck_block(block),
            ast::Expr::If {
                cond,
                then_block,
                else_block,
                span,
            } => {
                let cond_ty = self.typecheck_expr(cond)?;
                if cond_ty != Ty::Bool {
                    return Err(ParseError {
                        message: format!(
                            "`if` condition must be `bool`, found `{}`",
                            cond_ty.name()
                        ),
                        span: cond.span(),
                    });
                }
                let then_ty = self.typecheck_block(then_block)?;
                match else_block {
                    Some(else_block) => {
                        let else_ty = self.typecheck_block(else_block)?;
                        if then_ty != else_ty {
                            return Err(ParseError {
                                message: format!(
                                    "branches of `if` have different types: `{}` vs `{}`",
                                    then_ty.name(),
                                    else_ty.name()
                                ),
                                span: *span,
                            });
                        }
                        Ok(then_ty)
                    }
                    None => {
                        if then_ty != Ty::Unit {
                            return Err(ParseError {
                                message: format!(
                                    "`if` without `else` has type `()`; cannot use as a value of type `{}`",
                                    then_ty.name()
                                ),
                                span: *span,
                            });
                        }
                        Ok(Ty::Unit)
                    }
                }
            }
            // **M06**: borrow expressions `&place` and `&mut place`.
            // **M07.1**: peephole — `&v[range]` (range index inside a `&`) produces
            // `Ty::Slice(T)` directly, absorbing the leading `&` into the slice
            // type (matches Rust's `&[T]` shape). Detected structurally here so
            // the normal Borrow → Ref wrap doesn't fire.
            ast::Expr::Borrow { inner, mutable, span } => {
                if let ast::Expr::Index { receiver, index, span: idx_span } = inner.as_ref()
                    && matches!(index.as_ref(), ast::Expr::Range { .. })
                {
                    return self.typecheck_slice_borrow(
                        receiver,
                        index,
                        *mutable,
                        *idx_span,
                        *span,
                    );
                }
                self.typecheck_borrow(inner, *mutable, *span)
            }
            // **M06.1**: deref expression `*r`. Inner must be a reference;
            // the deref's type is the referenced type (regardless of mut).
            ast::Expr::Deref { inner, .. } => {
                let inner_ty = self.typecheck_expr(inner)?;
                match inner_ty {
                    Ty::Ref { inner: target, .. } => Ok(*target),
                    // M07: `*b` where b: Box<T> also derefs to T (auto-deref simplification).
                    Ty::Box(inner) => Ok(*inner),
                    other => Err(ParseError {
                        message: format!(
                            "cannot dereference value of type `{}`; expected a reference",
                            other.name()
                        ),
                        span: inner.span(),
                    }),
                }
            }
            // **M07 → M07.2**: string literal. M07 modeled this as `Ty::String`
            // (heap-owned) for typeck simplicity — wrong by Rust's semantics
            // since `"hi"` is `&'static str`, a borrow into the RO data
            // segment. M07.2 fixes it: literals are now `Ty::Str`.
            ast::Expr::StrLit(_, _) => Ok(Ty::Str),
            // **M07**: path expression (Vec::new, Box::new, String::from). These
            // ARE callable identifiers; when invoked via Expr::Call the call
            // arm consults the path-fn dispatch table. Bare path (no Call) is
            // not a valid expression on its own in M07.
            ast::Expr::Path { span, .. } => Err(ParseError {
                message: "path expression must be called (e.g. `Vec::new()`, `Box::new(v)`)".into(),
                span: *span,
            }),
            // **M07**: method call — dispatched via `typecheck_method_call`.
            ast::Expr::MethodCall { receiver, name, args, span } => {
                let receiver_ty = self.typecheck_expr(receiver)?;
                self.typecheck_method_call(&receiver_ty, name, args, *span)
            }
            // **M07**: indexing — receiver must be Vec, index must be Int.
            // **M07.1**: a range index inside a bare `v[range]` (no leading `&`)
            // is not a usable expression — Rust requires `&v[range]` to produce
            // a slice. Reject with a clear message pointing the user at `&`.
            ast::Expr::Index { receiver, index, span } => {
                if matches!(index.as_ref(), ast::Expr::Range { .. }) {
                    return Err(ParseError {
                        message: "range indexing produces an unsized slice; prefix with `&` to take a slice (e.g. `&v[1..3]`)".into(),
                        span: *span,
                    });
                }
                let receiver_ty = self.typecheck_expr(receiver)?;
                let index_ty = self.typecheck_expr(index)?;
                let elem_ty = match receiver_ty {
                    Ty::Vec(inner) => *inner,
                    // M07.3: array receiver — same result type as Vec.
                    Ty::Array(inner, _) => *inner,
                    other => {
                        return Err(ParseError {
                            message: format!("cannot index value of type `{}`; expected Vec or array", other.name()),
                            span: receiver.span(),
                        });
                    }
                };
                if !matches!(index_ty, Ty::Int(_)) {
                    return Err(ParseError {
                        message: format!("expected integer index, found `{}`", index_ty.name()),
                        span: index.span(),
                    });
                }
                let _ = span;
                Ok(elem_ty)
            }
            // **M07.1**: standalone range expression. Only valid inside an
            // `Expr::Index.index` position, and that path is handled
            // structurally above (Index + Borrow peek for Range). Any Range
            // reaching here is being used as a standalone expression.
            ast::Expr::Range { span, .. } => Err(ParseError {
                message: "range expressions are only valid inside index brackets in M07.1".into(),
                span: *span,
            }),
            // **M07.3**: array literal `[e1, e2, ..., eN]`. All elements
            // must unify to a common type via `try_coerce_to`. Result:
            // `Ty::Array(elem_ty, elements.len())`.
            ast::Expr::ArrayLit { elements, span } => self.typecheck_array_lit(elements, *span),
            // **M07.4**: struct literal — verify path resolves to a known
            // struct; verify every declared field appears with the right
            // type; reject extras.
            ast::Expr::StructLit { path, fields, span } => {
                self.typecheck_struct_lit(path, fields, *span)
            }
            // **M07.4**: field access on a struct (or auto-deref a &Struct).
            ast::Expr::FieldAccess { receiver, name, span } => {
                self.typecheck_field_access(receiver, name, *span)
            }
        }
    }

    /// **M07.4**: typecheck `Path { f1: e1, ... }`. Verifies the path
    /// resolves to a struct, every declared field appears exactly once,
    /// no extras, and each value's type matches the declared field type
    /// (with M03.2 literal-narrowing via `try_coerce_to`). Shorthand
    /// fields resolve to the bound local of the same name.
    fn typecheck_struct_lit(
        &mut self,
        path: &[String],
        fields: &[ast::StructLitField],
        span: Span,
    ) -> Result<Ty, ParseError> {
        if path.len() != 1 {
            return Err(ParseError {
                message: "M07.4 supports single-segment struct paths only".into(),
                span,
            });
        }
        let struct_name = &path[0];
        let schema = match self.structs.schemas.get(struct_name) {
            Some(s) => s.clone(),
            None => {
                return Err(ParseError {
                    message: format!("unknown struct `{struct_name}`"),
                    span,
                });
            }
        };
        // Pass 1: verify no extras. Each provided field name must exist in
        // the schema.
        for field in fields {
            if !schema.iter().any(|(n, _)| n == &field.name) {
                return Err(ParseError {
                    message: format!(
                        "no field `{}` on struct `{}`",
                        field.name, struct_name
                    ),
                    span: field.span,
                });
            }
        }
        // Pass 2: for each declared field, find the matching init OR
        // shorthand. Report missing fields.
        for (declared_name, declared_ty) in &schema {
            let init = fields.iter().find(|f| &f.name == declared_name);
            let Some(init) = init else {
                return Err(ParseError {
                    message: format!(
                        "missing field `{}` in struct literal `{}`",
                        declared_name, struct_name
                    ),
                    span,
                });
            };
            // Determine the field's typecheck-result and coerce to the
            // declared type. For shorthand (value: None), the bound local
            // of the same name is the source.
            let (value_ty, value_span, expr_for_coerce): (Ty, Span, Option<&ast::Expr>) =
                match &init.value {
                    Some(expr) => (self.typecheck_expr(expr)?, expr.span(), Some(expr)),
                    None => {
                        // Shorthand: resolve via the field's span (which
                        // resolve.rs registered as the use site for the
                        // bare ident).
                        let bid = *self
                            .resolution
                            .uses
                            .get(&init.span)
                            .expect("shorthand resolved in resolve.rs");
                        let bty = match self.types.binding_types.get(&bid) {
                            Some(BindingType::Var(t)) => t.clone(),
                            _ => panic!(
                                "shorthand binding has no Var type — resolve invariant"
                            ),
                        };
                        (bty, init.span, None)
                    }
                };
            let coerced = match expr_for_coerce {
                Some(e) => self
                    .try_coerce_to(e, value_ty.clone(), declared_ty.clone())
                    .unwrap_or(value_ty),
                None => value_ty,
            };
            if coerced != *declared_ty {
                return Err(ParseError {
                    message: format!(
                        "expected `{}`, found `{}`",
                        declared_ty.name(),
                        coerced.name()
                    ),
                    span: value_span,
                });
            }
        }
        // Duplicate-field check (`Point { x: 1, x: 2 }` — caught by the
        // first scan finding a second match below the first one in pass 2's
        // search, but easier to surface explicitly).
        for (i, field) in fields.iter().enumerate() {
            if fields[..i].iter().any(|f| f.name == field.name) {
                return Err(ParseError {
                    message: format!(
                        "field `{}` specified more than once",
                        field.name
                    ),
                    span: field.span,
                });
            }
        }
        Ok(Ty::Struct {
            name: struct_name.clone(),
            fields: schema,
        })
    }

    /// **M07.4**: typecheck `receiver.name`. Receiver must be `Ty::Struct(_)`
    /// or `Ty::Ref { Ty::Struct(_), .. }` (auto-deref). Field name must
    /// exist in the struct's schema. Multi-level access (`p.x.y`) rejected
    /// in M07.4.
    fn typecheck_field_access(
        &mut self,
        receiver: &ast::Expr,
        name: &str,
        span: Span,
    ) -> Result<Ty, ParseError> {
        // Reject multi-level: receiver must NOT be a FieldAccess itself.
        if matches!(receiver, ast::Expr::FieldAccess { .. }) {
            return Err(ParseError {
                message:
                    "nested struct fields not supported in M07.4 — use an intermediate let binding"
                        .into(),
                span,
            });
        }
        let receiver_ty = self.typecheck_expr(receiver)?;
        let schema = match &receiver_ty {
            Ty::Struct { fields, .. } => fields.clone(),
            Ty::Ref { inner, .. } => match inner.as_ref() {
                Ty::Struct { fields, .. } => fields.clone(),
                other => {
                    return Err(ParseError {
                        message: format!(
                            "field access requires a struct receiver, found `&{}`",
                            other.name()
                        ),
                        span: receiver.span(),
                    });
                }
            },
            other => {
                return Err(ParseError {
                    message: format!(
                        "field access requires a struct receiver, found `{}`",
                        other.name()
                    ),
                    span: receiver.span(),
                });
            }
        };
        match schema.iter().find(|(n, _)| n == name) {
            Some((_, ty)) => Ok(ty.clone()),
            None => {
                let struct_name = match &receiver_ty {
                    Ty::Struct { name, .. } => name.clone(),
                    Ty::Ref { inner, .. } => match inner.as_ref() {
                        Ty::Struct { name, .. } => name.clone(),
                        _ => "<unknown>".to_owned(),
                    },
                    _ => "<unknown>".to_owned(),
                };
                Err(ParseError {
                    message: format!("no field `{name}` on struct `{struct_name}`"),
                    span,
                })
            }
        }
    }

    /// **M07.3**: typecheck `[e1, e2, ..., eN]`. The first element anchors
    /// the type; subsequent elements coerce to it. Empty literal requires
    /// a separate annotation-driven path (typeck errors here without it).
    fn typecheck_array_lit(
        &mut self,
        elements: &[ast::Expr],
        span: Span,
    ) -> Result<Ty, ParseError> {
        if elements.is_empty() {
            return Err(ParseError {
                message:
                    "cannot infer element type for empty array literal — add a type annotation like `let t: [i32; 0] = [];`"
                        .into(),
                span,
            });
        }
        // Typecheck all elements once, collecting their types (and recording
        // their defaulted-or-explicit type on the spans). Untyped literals
        // get defaulted (i32 / f64) at this stage.
        let element_types: Vec<Ty> = elements
            .iter()
            .map(|el| self.typecheck_expr(el))
            .collect::<Result<_, _>>()?;
        // **Anchor selection**: an array literal's type is driven by the
        // first explicitly-typed element (a suffixed integer/float
        // literal, or any non-literal expression that brings its own
        // type). Untyped literals follow — they coerce to the anchor via
        // `try_coerce_to`. This mirrors Rust's actual inference:
        // `[10, 20, 30_u64]` infers `[u64; 3]` because `30_u64` is the
        // type-source; the `10` and `20` literal-narrow to u64. Without
        // this lookup, we'd anchor on `10`'s defaulted i32 and reject
        // the explicit u64 element with a confusing type-mismatch error.
        let is_explicit = |el: &ast::Expr| -> bool {
            match el {
                ast::Expr::LitInt(_, suffix, _) => suffix.is_some(),
                ast::Expr::LitFloat(_, suffix, _) => suffix.is_some(),
                ast::Expr::LitBool(_, _) => true, // bool has no defaulted form
                _ => true, // any non-literal expression has a concrete type
            }
        };
        let anchor_idx = elements
            .iter()
            .position(is_explicit)
            .unwrap_or(0);
        let anchor_ty = element_types[anchor_idx].clone();
        // Coerce + verify each remaining element against the anchor.
        for (i, el) in elements.iter().enumerate() {
            if i == anchor_idx {
                continue;
            }
            let el_ty = element_types[i].clone();
            let coerced = self
                .try_coerce_to(el, el_ty.clone(), anchor_ty.clone())
                .unwrap_or(el_ty);
            if coerced != anchor_ty {
                return Err(ParseError {
                    message: format!(
                        "array elements must all have the same type, found `{}` (expected `{}`)",
                        coerced.name(),
                        anchor_ty.name(),
                    ),
                    span: el.span(),
                });
            }
        }
        Ok(Ty::Array(Box::new(anchor_ty), elements.len() as u64))
    }

    /// **M07.1**: typecheck a slice borrow `&v[range]` (or rejected `&mut v[range]`).
    /// Returns `Ty::Slice(elem_ty)` — the leading `&` is absorbed into the slice
    /// type per Rust's `&[T]` semantics. Records the slice type on the index
    /// expression's span (so eval can recover it).
    fn typecheck_slice_borrow(
        &mut self,
        receiver: &ast::Expr,
        index: &ast::Expr,
        mutable: bool,
        idx_span: Span,
        borrow_span: Span,
    ) -> Result<Ty, ParseError> {
        if mutable {
            return Err(ParseError {
                message: "mutable slices are out of scope in M07.1 — only &[T] (shared) is supported".into(),
                span: borrow_span,
            });
        }
        // Receiver: Vec<T> (M07.1), &[T] (M07.2 — slice-of-slice), or &str
        // (M07.2 — sub-slicing a string literal). The result type preserves
        // the receiver's "shape": slicing a Vec or a &[T] yields &[T];
        // slicing a &str yields &str (the sugar is preserved).
        let receiver_ty = self.typecheck_expr(receiver)?;
        let (elem_ty, result_ty) = match receiver_ty {
            Ty::Vec(inner) => {
                let inner = *inner;
                let result = Ty::Slice(Box::new(inner.clone()));
                (inner, result)
            }
            // M07.2: slice-of-slice (forward-compat for any &[T] receiver).
            Ty::Slice(inner) => {
                let inner = *inner;
                let result = Ty::Slice(Box::new(inner.clone()));
                (inner, result)
            }
            // M07.2: sub-slicing a `&str` produces another `&str` (sugar
            // preserved). Underneath it's a slice of bytes.
            Ty::Str => (Ty::Int(IntKind::U8), Ty::Str),
            // **M07.3**: slicing an array `[T; N]` produces `&[T]` —
            // size is lost on the borrow (matches Rust; no `&[T; M]`
            // borrows in M07.3 scope).
            Ty::Array(inner, _) => {
                let inner = *inner;
                let result = Ty::Slice(Box::new(inner.clone()));
                (inner, result)
            }
            other => {
                return Err(ParseError {
                    message: format!(
                        "cannot slice value of type `{}`; expected Vec, &[T], &str, or array",
                        other.name()
                    ),
                    span: receiver.span(),
                });
            }
        };
        // Range bounds must be integer.
        let (start, end) = match index {
            ast::Expr::Range { start, end, .. } => (start.as_deref(), end.as_deref()),
            _ => unreachable!("caller guarantees Range"),
        };
        for bound in [start, end].iter().flatten() {
            let bound_ty = self.typecheck_expr(bound)?;
            if !matches!(bound_ty, Ty::Int(_)) {
                return Err(ParseError {
                    message: format!(
                        "range bound must be integer, found `{}`",
                        bound_ty.name()
                    ),
                    span: bound.span(),
                });
            }
        }
        // Record the slice type on the inner Index span so eval can confirm
        // the slice-borrow shape; also on the Borrow span (caller does this).
        // M07.2: `result_ty` preserves the receiver's shape — Vec/&[T] → &[T],
        // &str → &str. `elem_ty` only feeds future bounds/dispatch checks.
        let _ = elem_ty;
        self.types.expr_types.insert(idx_span, result_ty.clone());
        Ok(result_ty)
    }

    /// **M07**: dispatch a path-fn call against the hardcoded static-fn table.
    /// Recognized paths: `Box::new(v) -> Box<T>`, `Vec::new() -> Vec<T>`,
    /// `String::from(s: StrLit) -> String`.
    fn typecheck_path_call(
        &mut self,
        segments: &[String],
        path_span: Span,
        args: &[ast::Expr],
        call_span: Span,
    ) -> Result<Ty, ParseError> {
        let seg_strs: Vec<&str> = segments.iter().map(|s| s.as_str()).collect();
        match seg_strs.as_slice() {
            ["Box", "new"] => {
                if args.len() != 1 {
                    return Err(ParseError {
                        message: format!("Box::new takes 1 arg, found {}", args.len()),
                        span: call_span,
                    });
                }
                let arg_ty = self.typecheck_expr(&args[0])?;
                Ok(Ty::Box(Box::new(arg_ty)))
            }
            ["Vec", "new"] => {
                if !args.is_empty() {
                    return Err(ParseError {
                        message: "Vec::new takes no args".into(),
                        span: call_span,
                    });
                }
                // Type inference: Vec::new()'s T comes from the let-annotation
                // (handled at the let-stmt level via the annotation match).
                // Here we return a placeholder `Vec<Unit>` that the caller
                // overrides via try_coerce_to or the let-stmt annotation path.
                // Simpler approach: require annotation on the let binding by
                // returning Ty::Vec(Box::new(Ty::Unit)) as a sentinel; the
                // typecheck_stmt let-arm coerces it. Plan-phase R-013.
                //
                // M07 simplification: peek up the AST to find the enclosing
                // let-annotation isn't easy here, so we use a Ty::Vec(Unit)
                // sentinel and rely on the let-stmt's annotation comparison
                // to override (via a new coercion case below).
                Ok(Ty::Vec(Box::new(Ty::Unit)))
            }
            ["String", "from"] => {
                if args.len() != 1 {
                    return Err(ParseError {
                        message: format!("String::from takes 1 arg, found {}", args.len()),
                        span: call_span,
                    });
                }
                // M07.2: accept any `&str` arg (literal OR an existing
                // `&str` binding OR a sub-slice). Eval extracts bytes from
                // the slice's static region using its byte_offset/byte_len.
                let arg_ty = self.typecheck_expr(&args[0])?;
                if !matches!(arg_ty, Ty::Str)
                    && !matches!(&arg_ty, Ty::Slice(inner) if matches!(**inner, Ty::Int(IntKind::U8)))
                {
                    return Err(ParseError {
                        message: format!(
                            "String::from: expected `&str`, found `{}`",
                            arg_ty.name()
                        ),
                        span: args[0].span(),
                    });
                }
                Ok(Ty::String)
            }
            _ => {
                // **M07.4**: fall through to user-defined associated functions.
                let key: Vec<String> = segments.to_vec();
                if let Some(sig) = self.impls.assoc_fns.get(&key).cloned() {
                    if args.len() != sig.params.len() {
                        return Err(ParseError {
                            message: format!(
                                "associated fn `{}` expects {} argument(s), found {}",
                                segments.join("::"),
                                sig.params.len(),
                                args.len()
                            ),
                            span: call_span,
                        });
                    }
                    for (i, (arg, param_ty)) in args.iter().zip(sig.params.iter()).enumerate() {
                        let arg_ty = self.typecheck_expr(arg)?;
                        let arg_ty = self
                            .try_coerce_to(arg, arg_ty.clone(), param_ty.clone())
                            .unwrap_or(arg_ty);
                        if arg_ty != *param_ty {
                            return Err(ParseError {
                                message: format!(
                                    "argument {}: expected `{}`, found `{}`",
                                    i + 1,
                                    param_ty.name(),
                                    arg_ty.name()
                                ),
                                span: arg.span(),
                            });
                        }
                    }
                    return Ok(sig.ret);
                }
                Err(ParseError {
                    message: format!("unknown path `{}`", segments.join("::")),
                    span: path_span,
                })
            }
        }
    }

    /// **M07**: dispatch a method call against the hardcoded structural table.
    fn typecheck_method_call(
        &mut self,
        receiver_ty: &Ty,
        name: &str,
        args: &[ast::Expr],
        span: Span,
    ) -> Result<Ty, ParseError> {
        match (receiver_ty, name) {
            (Ty::Vec(elem_ty), "push") => {
                if args.len() != 1 {
                    return Err(ParseError {
                        message: format!("Vec::push takes 1 arg, found {}", args.len()),
                        span,
                    });
                }
                let arg_ty = self.typecheck_expr(&args[0])?;
                // Allow literal coercion to the Vec's element type.
                let arg_ty = self
                    .try_coerce_to(&args[0], arg_ty.clone(), (**elem_ty).clone())
                    .unwrap_or(arg_ty);
                if arg_ty != **elem_ty {
                    return Err(ParseError {
                        message: format!(
                            "Vec::push: expected `{}`, found `{}`",
                            elem_ty.name(),
                            arg_ty.name()
                        ),
                        span: args[0].span(),
                    });
                }
                Ok(Ty::Unit)
            }
            (Ty::Vec(_), "len") => {
                if !args.is_empty() {
                    return Err(ParseError {
                        message: "Vec::len takes no args".into(),
                        span,
                    });
                }
                Ok(Ty::Int(IntKind::U64))
            }
            // M07.1: `Slice::len() -> u64`. Same signature as Vec::len.
            // M07.2: `&str` is a sugar for `&[u8]`, so the same `len()` works.
            // M07.3: `[T; N]::len()` returns N (compile-time constant).
            (Ty::Slice(_), "len") | (Ty::Str, "len") | (Ty::Array(_, _), "len") => {
                if !args.is_empty() {
                    return Err(ParseError {
                        message: "len takes no args".into(),
                        span,
                    });
                }
                Ok(Ty::Int(IntKind::U64))
            }
            (Ty::String, "push_str") => {
                if args.len() != 1 {
                    return Err(ParseError {
                        message: format!("String::push_str takes 1 arg, found {}", args.len()),
                        span,
                    });
                }
                // M07.2: accept any `&str` arg (literal, binding, or sub-slice).
                let arg_ty = self.typecheck_expr(&args[0])?;
                if !matches!(arg_ty, Ty::Str)
                    && !matches!(&arg_ty, Ty::Slice(inner) if matches!(**inner, Ty::Int(IntKind::U8)))
                {
                    return Err(ParseError {
                        message: format!(
                            "String::push_str: expected `&str`, found `{}`",
                            arg_ty.name()
                        ),
                        span: args[0].span(),
                    });
                }
                Ok(Ty::Unit)
            }
            _ => {
                // **M07.4**: fall through to user-defined methods. Hardcoded
                // built-ins above always win (R-018 tie-breaker). Auto-deref:
                // `&T` / `&mut T` receivers dispatch as the underlying T.
                let receiver_struct_name = match receiver_ty {
                    Ty::Struct { name, .. } => Some(name.clone()),
                    Ty::Ref { inner, .. } => match inner.as_ref() {
                        Ty::Struct { name, .. } => Some(name.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(struct_name) = receiver_struct_name {
                    let key = (struct_name.clone(), name.to_owned());
                    if let Some(sig) = self.impls.methods.get(&key).cloned() {
                        if args.len() != sig.params.len() {
                            return Err(ParseError {
                                message: format!(
                                    "method `{}` on `{struct_name}` expects {} argument(s), found {}",
                                    name,
                                    sig.params.len(),
                                    args.len()
                                ),
                                span,
                            });
                        }
                        for (i, (arg, param_ty)) in args.iter().zip(sig.params.iter()).enumerate()
                        {
                            let arg_ty = self.typecheck_expr(arg)?;
                            let arg_ty = self
                                .try_coerce_to(arg, arg_ty.clone(), param_ty.clone())
                                .unwrap_or(arg_ty);
                            if arg_ty != *param_ty {
                                return Err(ParseError {
                                    message: format!(
                                        "argument {}: expected `{}`, found `{}`",
                                        i + 1,
                                        param_ty.name(),
                                        arg_ty.name()
                                    ),
                                    span: arg.span(),
                                });
                            }
                        }
                        return Ok(sig.ret);
                    }
                }
                Err(ParseError {
                    message: format!(
                        "no method `{name}` on type `{}`",
                        receiver_ty.name()
                    ),
                    span,
                })
            }
        }
    }

    /// **M06**: typecheck a borrow expression. Verifies inner is a place
    /// expression (Ident only in L2), takes a borrow via the borrow tracker
    /// (enforcing aliasing rules), and returns `Ty::Ref { inner, mutable }`.
    fn typecheck_borrow(
        &mut self,
        inner: &ast::Expr,
        mutable: bool,
        span: Span,
    ) -> Result<Ty, ParseError> {
        // Place-expression check: Ident (M06), Index of an Ident (M07: `&v[i]`),
        // or FieldAccess on an Ident (M07.4: `&p.x`).
        let target_binding = match inner {
            ast::Expr::Ident(_, sp) => *self.resolution.uses.get(sp).expect("ident resolved"),
            // **M07**: `&v[i]` borrows the Vec's heap allocation. The target
            // binding for mut-checking + tracker is `v`.
            ast::Expr::Index { receiver, .. } => match receiver.as_ref() {
                ast::Expr::Ident(_, sp) => *self.resolution.uses.get(sp).expect("ident resolved"),
                _ => return Err(ParseError {
                    message: "expected `&place[index]` with a binding name as the receiver".into(),
                    span: receiver.span(),
                }),
            },
            // **M07.4**: `&p.x` borrows a sub-field of a struct binding.
            // The target binding (for mut-check + borrow-tracker) is the
            // receiver of the field access; multi-level paths (`&p.x.y`)
            // are rejected to keep M07.4 scope tight.
            ast::Expr::FieldAccess { receiver, name: field_name, span: fa_span } => {
                let recv_binding = match receiver.as_ref() {
                    ast::Expr::Ident(_, sp) => {
                        *self.resolution.uses.get(sp).expect("ident resolved")
                    }
                    _ => {
                        return Err(ParseError {
                            message: "expected `&place.field` with a binding name as the receiver"
                                .into(),
                            span: receiver.span(),
                        });
                    }
                };
                // Verify the receiver's type is a struct AND the field exists.
                // We let the regular typecheck_field_access path handle the
                // error messages by typechecking the inner expression (it
                // catches both non-struct receivers and unknown fields).
                let inner_ty = self.typecheck_field_access(receiver, field_name, *fa_span)?;
                self.types.expr_types.insert(*fa_span, inner_ty.clone());
                // For `&mut p.x` the receiver binding must be `mut`. We
                // intentionally don't track per-field aliasing — a `&p.x`
                // takes a shared borrow on `p`, and the borrow tracker
                // catches obvious aliasing conflicts at the binding level.
                if mutable {
                    let target_decl = &self.resolution.bindings[&recv_binding];
                    let is_mut_let = matches!(
                        target_decl.kind,
                        BindingKind::Let { mutable: true, .. },
                    );
                    if !is_mut_let {
                        let name = target_decl.name.clone();
                        return Err(ParseError {
                            message: format!(
                                "cannot borrow `{name}` as mutable; it is not declared as `mut`"
                            ),
                            span,
                        });
                    }
                }
                // Take the borrow on the receiver binding.
                let depth = self.scope_depth;
                let check = if mutable {
                    self.borrow_tracker.try_take_mut(recv_binding, depth, span)
                } else {
                    self.borrow_tracker.try_take_shared(recv_binding, depth, span)
                };
                if let Err(conflict) = check {
                    let target_name =
                        self.resolution.bindings[&recv_binding].name.clone();
                    return Err(ParseError {
                        message: format!(
                            "cannot borrow `{target_name}` as {new_kind} because it is already borrowed as {existing_kind}",
                            new_kind = if mutable { "mutable" } else { "immutable" },
                            existing_kind = match conflict.existing_kind {
                                borrow_tracker::BorrowKind::Shared => "immutable",
                                borrow_tracker::BorrowKind::Mut => "mutable",
                            }
                        ),
                        span,
                    });
                }
                return Ok(Ty::Ref {
                    inner: Box::new(inner_ty),
                    mutable,
                });
            }
            other => {
                return Err(ParseError {
                    message: "expected place expression for borrow (identifier, `&place[index]`, or `&place.field`)".into(),
                    span: other.span(),
                });
            }
        };
        let target_decl = &self.resolution.bindings[&target_binding];
        let target_name = target_decl.name.clone();
        // For `&mut x`, the binding must be declared `mut`. (Skip for `&v[i]`
        // — M07 simplification: heap-element mutable borrows out of scope.)
        if mutable && matches!(inner, ast::Expr::Ident(_, _)) {
            let is_mut_let = matches!(
                target_decl.kind,
                BindingKind::Let { mutable: true, .. },
            );
            // Function parameters are not `mut` in our L1; treat as non-mut.
            if !is_mut_let {
                return Err(ParseError {
                    message: format!(
                        "cannot borrow `{target_name}` as mutable; it is not declared as `mut`"
                    ),
                    span,
                });
            }
        }
        // Typecheck the inner expression to get T.
        let inner_ty = self.typecheck_expr(inner)?;
        // Aliasing check via the borrow tracker.
        let depth = self.scope_depth;
        let check = if mutable {
            self.borrow_tracker.try_take_mut(target_binding, depth, span)
        } else {
            self.borrow_tracker
                .try_take_shared(target_binding, depth, span)
        };
        if let Err(conflict) = check {
            return Err(ParseError {
                message: format!(
                    "cannot borrow `{target_name}` as {new_kind} because it is already borrowed as {existing_kind}",
                    new_kind = if mutable { "mutable" } else { "immutable" },
                    existing_kind = match conflict.existing_kind {
                        borrow_tracker::BorrowKind::Shared => "immutable",
                        borrow_tracker::BorrowKind::Mut => "mutable",
                    }
                ),
                span,
            });
        }
        Ok(Ty::Ref {
            inner: Box::new(inner_ty),
            mutable,
        })
    }

    fn typecheck_binary(
        &mut self,
        op: ast::BinOp,
        lhs: &ast::Expr,
        rhs: &ast::Expr,
        span: Span,
    ) -> Result<Ty, ParseError> {
        let lhs_ty = self.typecheck_expr(lhs)?;
        let rhs_ty = self.typecheck_expr(rhs)?;
        use ast::BinOp::*;
        match op {
            Add | Sub | Mul | Div | Rem => {
                let (lhs_ty, rhs_ty) = self.unify_numeric_operands(lhs, rhs, lhs_ty, rhs_ty);
                let unified = match (&lhs_ty, &rhs_ty) {
                    (Ty::Int(a), Ty::Int(b)) if a == b => Ty::Int(*a),
                    (Ty::Float(a), Ty::Float(b)) if a == b => Ty::Float(*a),
                    _ => {
                        return Err(ParseError {
                            message: format!(
                                "binary operator `{}` requires both operands to be the same numeric type, found `{}` and `{}`",
                                binop_str(op),
                                lhs_ty.name(),
                                rhs_ty.name()
                            ),
                            span,
                        });
                    }
                };
                Ok(unified)
            }
            Lt | Le | Gt | Ge => {
                let (lhs_ty, rhs_ty) = self.unify_numeric_operands(lhs, rhs, lhs_ty, rhs_ty);
                let ok = matches!((&lhs_ty, &rhs_ty),
                    (Ty::Int(a), Ty::Int(b)) if a == b)
                    || matches!((&lhs_ty, &rhs_ty),
                        (Ty::Float(a), Ty::Float(b)) if a == b);
                if !ok {
                    return Err(ParseError {
                        message: format!(
                            "comparison operator `{}` requires both operands to be the same numeric type, found `{}` and `{}`",
                            binop_str(op),
                            lhs_ty.name(),
                            rhs_ty.name()
                        ),
                        span,
                    });
                }
                Ok(Ty::Bool)
            }
            Eq | Neq => {
                if lhs_ty != rhs_ty {
                    return Err(ParseError {
                        message: format!(
                            "equality operator `{}` requires both operands to be the same type, found `{}` and `{}`",
                            binop_str(op),
                            lhs_ty.name(),
                            rhs_ty.name()
                        ),
                        span,
                    });
                }
                if lhs_ty == Ty::Unit {
                    return Err(ParseError {
                        message: format!(
                            "equality operator `{}` cannot compare values of type `()`",
                            binop_str(op)
                        ),
                        span,
                    });
                }
                Ok(Ty::Bool)
            }
            And | Or => {
                if lhs_ty != Ty::Bool || rhs_ty != Ty::Bool {
                    return Err(ParseError {
                        message: format!(
                            "logical operator `{}` requires both operands to be `bool`, found `{}` and `{}`",
                            binop_str(op),
                            lhs_ty.name(),
                            rhs_ty.name()
                        ),
                        span,
                    });
                }
                Ok(Ty::Bool)
            }
        }
    }

    fn typecheck_call(
        &mut self,
        callee: &ast::Expr,
        args: &[ast::Expr],
        call_span: Span,
    ) -> Result<Ty, ParseError> {
        // **M07**: Path-callee → dispatch the static-fn table (Box::new, Vec::new, String::from).
        if let ast::Expr::Path { segments, span: path_span } = callee {
            return self.typecheck_path_call(segments, *path_span, args, call_span);
        }
        // L1 supports direct function calls (callee must be an Ident).
        let (callee_name, callee_span) = match callee {
            ast::Expr::Ident(name, sp) => (name.clone(), *sp),
            _ => {
                return Err(ParseError {
                    message: "callee must be a function name or path (e.g. `Box::new(v)`)".into(),
                    span: callee.span(),
                });
            }
        };
        let id = *self
            .resolution
            .uses
            .get(&callee_span)
            .expect("callee ident resolved");
        let sig = match self.types.binding_types.get(&id) {
            Some(BindingType::Fn(s)) => s.clone(),
            Some(BindingType::Var(_)) => {
                return Err(ParseError {
                    message: format!("`{callee_name}` is not a function"),
                    span: callee_span,
                });
            }
            None => panic!("binding {id:?} has no type"),
        };
        // NB: do NOT record `expr_types[callee_span]` — the callee is a function
        // reference, not a value (data-model.md VR-11).
        if args.len() != sig.params.len() {
            return Err(ParseError {
                message: format!(
                    "function `{callee_name}` expects {} argument(s), found {}",
                    sig.params.len(),
                    args.len()
                ),
                span: call_span,
            });
        }
        for (i, (arg, param_ty)) in args.iter().zip(sig.params.iter()).enumerate() {
            let arg_ty = self.typecheck_expr(arg)?;
            if arg_ty != *param_ty {
                return Err(ParseError {
                    message: format!(
                        "argument {}: expected `{}`, found `{}`",
                        i + 1,
                        param_ty.name(),
                        arg_ty.name()
                    ),
                    span: arg.span(),
                });
            }
        }
        Ok(sig.ret)
    }

    /// **M03.2**: attempt to coerce a literal expression's type to `target`.
    /// Currently handles `Expr::LitInt(n)` → `Ty::Int(k)` when `k.contains(n)`,
    /// and `Expr::Unary { Neg, LitInt }` → `Ty::Int(k)` when signed `k` fits
    /// the negated literal. Returns `Some(target)` on successful coercion
    /// (and updates the recorded expression type), `None` otherwise.
    fn try_coerce_to(&mut self, expr: &ast::Expr, current: Ty, target: Ty) -> Option<Ty> {
        if current == target {
            return Some(target);
        }
        // **M07**: `Vec::new()` typechecks to the sentinel `Ty::Vec(Box::new(Ty::Unit))`;
        // the surrounding let-annotation provides the real element type.
        if let Ty::Vec(inner) = &current {
            if let Ty::Vec(target_inner) = &target {
                if **inner == Ty::Unit {
                    // Override the placeholder Vec<Unit> with the annotation's element type.
                    let span = expr.span();
                    let new_ty = Ty::Vec(target_inner.clone());
                    self.types.expr_types.insert(span, new_ty.clone());
                    return Some(new_ty);
                }
            }
        }
        match (expr, target) {
            // Suffixed literal: don't coerce, the kind is locked in by syntax.
            (ast::Expr::LitInt(_, Some(_), _), _) => None,
            (ast::Expr::LitFloat(_, Some(_), _), _) => None,
            (ast::Expr::LitInt(n, None, span), Ty::Int(k)) => {
                if k.contains(*n as i128) {
                    self.types.expr_types.insert(*span, Ty::Int(k));
                    Some(Ty::Int(k))
                } else {
                    None
                }
            }
            // Integer literal annotated as float: `let x: f64 = 5;` is valid Rust.
            (ast::Expr::LitInt(_, None, span), Ty::Float(k)) => {
                self.types.expr_types.insert(*span, Ty::Float(k));
                Some(Ty::Float(k))
            }
            // Float literal coerces between f32/f64 freely (narrowing happens at eval).
            (ast::Expr::LitFloat(_, None, span), Ty::Float(k)) => {
                self.types.expr_types.insert(*span, Ty::Float(k));
                Some(Ty::Float(k))
            }
            (ast::Expr::Unary { op: ast::UnOp::Neg, expr: inner, span }, Ty::Int(k))
                if k.is_signed() =>
            {
                if let ast::Expr::LitInt(n, None, inner_span) = inner.as_ref() {
                    let negated = -(*n as i128);
                    if k.contains(negated) {
                        self.types.expr_types.insert(*inner_span, Ty::Int(k));
                        self.types.expr_types.insert(*span, Ty::Int(k));
                        return Some(Ty::Int(k));
                    }
                }
                None
            }
            // Unary `-` on a float literal: coerce the float to the target kind.
            (ast::Expr::Unary { op: ast::UnOp::Neg, expr: inner, span }, Ty::Float(k)) => {
                if let ast::Expr::LitFloat(_, None, inner_span) = inner.as_ref() {
                    self.types.expr_types.insert(*inner_span, Ty::Float(k));
                    self.types.expr_types.insert(*span, Ty::Float(k));
                    return Some(Ty::Float(k));
                }
                // Also allow unary `-` on an int literal annotated as float.
                if let ast::Expr::LitInt(_, None, inner_span) = inner.as_ref() {
                    self.types.expr_types.insert(*inner_span, Ty::Float(k));
                    self.types.expr_types.insert(*span, Ty::Float(k));
                    return Some(Ty::Float(k));
                }
                None
            }
            _ => None,
        }
    }

    /// **M03.2**: try to bring the two operands of a binary op to a common
    /// numeric type by coercing whichever side is a literal. If neither side
    /// is a literal (or coercion fails), returns the types unchanged — the
    /// caller will then issue a cross-type typeck error.
    fn unify_numeric_operands(
        &mut self,
        lhs: &ast::Expr,
        rhs: &ast::Expr,
        lhs_ty: Ty,
        rhs_ty: Ty,
    ) -> (Ty, Ty) {
        if lhs_ty == rhs_ty {
            return (lhs_ty, rhs_ty);
        }
        if let Some(new_rhs) = self.try_coerce_to(rhs, rhs_ty.clone(), lhs_ty.clone()) {
            return (lhs_ty, new_rhs);
        }
        if let Some(new_lhs) = self.try_coerce_to(lhs, lhs_ty.clone(), rhs_ty.clone()) {
            return (new_lhs, rhs_ty);
        }
        (lhs_ty, rhs_ty)
    }
}

fn ty_from_ast(t: &ast::Type) -> Result<Ty, ParseError> {
    match t {
        ast::Type::Unit { .. } => Ok(Ty::Unit),
        // **M06**: `&T` or `&mut T`. Inner is recursively resolved.
        ast::Type::Ref { inner, mutable, .. } => {
            let inner_ty = ty_from_ast(inner)?;
            Ok(Ty::Ref {
                inner: Box::new(inner_ty),
                mutable: *mutable,
            })
        }
        ast::Type::Path { segments, span } => {
            if segments.len() != 1 {
                return Err(ParseError {
                    message: "multi-segment type paths are not supported in L1".into(),
                    span: *span,
                });
            }
            match segments[0].as_str() {
                "i8" => Ok(Ty::Int(IntKind::I8)),
                "i16" => Ok(Ty::Int(IntKind::I16)),
                "i32" => Ok(Ty::Int(IntKind::I32)),
                "i64" => Ok(Ty::Int(IntKind::I64)),
                "i128" => Ok(Ty::Int(IntKind::I128)),
                "u8" => Ok(Ty::Int(IntKind::U8)),
                "u16" => Ok(Ty::Int(IntKind::U16)),
                "u32" => Ok(Ty::Int(IntKind::U32)),
                "u64" => Ok(Ty::Int(IntKind::U64)),
                "u128" => Ok(Ty::Int(IntKind::U128)),
                "isize" => Ok(Ty::Int(IntKind::ISize)),
                "usize" => Ok(Ty::Int(IntKind::USize)),
                "f32" => Ok(Ty::Float(FloatKind::F32)),
                "f64" => Ok(Ty::Float(FloatKind::F64)),
                "bool" => Ok(Ty::Bool),
                // **M07**: `String` as a bare path (no generics).
                "String" => Ok(Ty::String),
                other => Err(ParseError {
                    message: format!("unknown type `{other}`"),
                    span: *span,
                }),
            }
        }
        // **M07**: generic type paths `Box<T>`, `Vec<T>`. Validates segment
        // name + arity, recurses on the inner type.
        ast::Type::Generic { segments, args, span } => {
            if segments.len() != 1 || args.len() != 1 {
                return Err(ParseError {
                    message: "only single-segment generics with one type arg are supported".into(),
                    span: *span,
                });
            }
            let inner = ty_from_ast(&args[0])?;
            match segments[0].as_str() {
                "Box" => Ok(Ty::Box(Box::new(inner))),
                "Vec" => Ok(Ty::Vec(Box::new(inner))),
                other => Err(ParseError {
                    message: format!("unknown generic type `{other}`"),
                    span: *span,
                }),
            }
        }
        // **M07.1**: slice annotation `&[T]` / `&mut [T]`. M07.1 only supports
        // shared slices; `&mut [T]` is rejected here.
        ast::Type::Slice { inner, mutable, span } => {
            if *mutable {
                return Err(ParseError {
                    message: "mutable slices are out of scope in M07.1 — only &[T] (shared) is supported".into(),
                    span: *span,
                });
            }
            let inner_ty = ty_from_ast(inner)?;
            Ok(Ty::Slice(Box::new(inner_ty)))
        }
        // **M07.3**: array annotation `[T; N]`.
        ast::Type::Array { inner, size, .. } => {
            let inner_ty = ty_from_ast(inner)?;
            Ok(Ty::Array(Box::new(inner_ty), *size))
        }
    }
}

fn binop_str(op: ast::BinOp) -> &'static str {
    use ast::BinOp::*;
    match op {
        Add => "+",
        Sub => "-",
        Mul => "*",
        Div => "/",
        Rem => "%",
        Lt => "<",
        Le => "<=",
        Gt => ">",
        Ge => ">=",
        Eq => "==",
        Neq => "!=",
        And => "&&",
        Or => "||",
    }
}

fn unop_str(op: ast::UnOp) -> &'static str {
    match op {
        ast::UnOp::Neg => "-",
        ast::UnOp::Not => "!",
    }
}
