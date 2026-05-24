# Research — M07.6 Implementation Decisions

17 decisions across parser, AST, typeck (registries + bound checking + dispatch), eval (trait-method frames + default-method routing), UI (mangled name format), and protocol amendment.

## Parser

### R-001 — `trait Name { ... }` parsed by `parse_trait_decl` after `Trait` keyword

- **Decision**: `parse_item` peeks the leading keyword. `Fn` → existing `parse_fn_decl` (M01); `Struct` → `parse_struct_decl` (M07.4); `Impl` → `parse_impl_block` (M07.4); **`Trait` → new `parse_trait_decl`** (M07.6).
- `parse_trait_decl` consumes `trait`, expects ident (name), `{`, parses trait items, `}`. Trait items are fn decls — if the fn has a body, it's a default method; if the fn ends after `;` (no body), it's a required method.
- **Required-method syntax**: `fn show(&self) -> i32;` (semicolon after the return type, no body). Parser distinguishes by peeking `Semi` vs `LBrace` after `-> Ty`.
- **Rationale**: minimal new AST surface — `TraitItem` enum with `Required { name, params, return_ty, span }` and `Default { decl: FnDecl }` variants.

### R-002 — Trait impl `impl Show for Point { ... }` parsed by extending `parse_impl_block`

- **Decision**: `parse_impl_block` (M07.4) currently consumes `impl`, expects ident (type name), `{`, parses fn items. M07.6 extension: after `impl`, peek for `Ident for Ident` shape (the trait_name + `for` keyword + type_name). If `for` keyword present → trait impl; else inherent (existing M07.4).
- **`for` keyword**: needs lexer support if absent. Verify M07.4 didn't already add it (`for` loops aren't in any milestone yet, so `for` likely isn't a keyword — need to add).
- **Rationale**: minimal disambiguation via single-token lookahead (the `for` keyword).

### R-003 — Multi-bound `<T: A + B>` parsed by extending M07.5's bound consumption

- **Decision**: M07.5's `parse_type_params` captures at most one bound (`bound: Option<String>`). M07.6 extension: after consuming the first bound ident, loop while `Plus` token follows — consume `Plus`, consume next ident, push to bounds. Build `bounds: Vec<String>`.
- **`Plus` token**: already exists as binary-add op; parser context (inside type-param bound) disambiguates.
- **Rationale**: tiny extension; matches Rust syntax.

### R-004 — `TypeParam.bound` field promoted to `bounds: Vec<String>`

- **Decision**: rename + promote. M07.5 had `bound: Option<String>` (parser stored; typeck rejected). M07.6 has `bounds: Vec<String>` (empty = no bounds; non-empty = active bounds checked by typeck).
- **Migration cost**: M07.5's typeck-side rejection of `Some(bound)` becomes typeck-side acceptance + bound-checking in M07.6. The parser code that captured the bound stays similar.

## AST

### R-005 — `Item::Trait(TraitDecl)` + `TraitItem` enum

- **Decision**: new `Item` variant carrying the trait declaration.
- **`TraitDecl { name, items, span }`**: name = the trait's identifier; items = the list of fn decls inside.
- **`TraitItem` enum**: `Required { name, params, return_ty, span }` (signature-only) OR `Default { decl: FnDecl }` (with body). Self-receiver via `Param.kind` (M07.4 machinery).
- **Rationale**: matches the lexical structure; cleanly separates required vs default at the AST level.

### R-006 — `ImplBlock.trait_name: Option<String>` (extension)

- **Decision**: extend M07.4's `ImplBlock` with an optional `trait_name`. `None` = inherent impl (M07.4 behavior); `Some(name)` = trait impl.
- **Rationale**: minimal addition; existing M07.4 code paths still work for inherent impls (the new field defaults to None at every M07.4 construction site).

## Typeck

### R-007 — `TraitRegistry` (phase-1-built)

- **Decision**: `IndexMap<String, TraitSchema>` keyed by trait name. `TraitSchema { required_methods: IndexMap<String, FnSig>, default_methods: IndexMap<String, &'a ast::FnDecl> }`.
- **Why store FnDecl reference for defaults**: the body needs to be re-walked at typeck phase 2 (for trait-method-body typecheck) AND at eval time. Storing the reference avoids cloning.
- **Phase-1 collection**: iterate `program.items`; for each `Item::Trait`, register the schema. Walk required vs default methods; reject duplicates.

### R-008 — `TraitImplRegistry` (phase-1-built)

- **Decision**: `IndexMap<(String, String), TraitImpl>` keyed by `(trait_name, type_name)`. `TraitImpl { overrides: IndexMap<String, &'a ast::FnDecl> }`.
- **`overrides` semantics**: only methods that the impl provides explicitly. Default methods fall through to the trait schema at dispatch time.
- **Phase-1 validation**: for each `Item::Impl { trait_name: Some(_), .. }`, verify the trait exists; every method in the impl must be on the trait (reject "extra method"); every required method on the trait must have an impl override (reject "missing required method").
- **Default-method override is OPTIONAL**: an impl can choose to not override a default; dispatch falls through to the trait's body.

### R-009 — `TypeParam.bounds: Vec<String>` checked at call sites

- **Decision**: at a generic-fn call site, after inferring the substitution (M07.5 machinery), for each type-param `T` with bounds `[A, B]`, check that the substituted concrete type `T_concrete` has impls of both A AND B. Lookup: `trait_impls.contains_key(("A", T_concrete.name()))`.
- **Error message**: `"the trait bound `<T_concrete>: <Trait>` is not satisfied"` — match Rust phrasing for learner familiarity.

### R-010 — Method dispatch on type-param-typed values: bound proves the method

- **Decision**: inside a generic fn body, when `x` has type `Ty::Param("T")` and the call is `x.show()`:
  1. Look up `T`'s bounds in the current type-param scope.
  2. For each bound trait, check if it has a method named `show` (in `required_methods` or `default_methods`).
  3. First match wins. Ambiguity (multiple bounds, same method name) → error suggesting UFCS.
  4. Result type = the matched method's return type, with `Self` substituted by `Ty::Param("T")` (for M07.6's no-`Self`-return restriction, this should never come up).
- **Without a bound proving the method**: typeck error. `fn id<T>(x: T) { x.show(); }` (no bound) → "method `show` not found on type `T` (no traits in T's bounds declare `show`)".

### R-011 — Three-layer dispatch: builtins → inherent → trait

- **Decision**: extend `typecheck_method_call`'s fall-through chain. Order:
  1. Hardcoded built-ins (Vec::push, String::push_str, Slice::len, Array::len, Vec::len — M07 + M07.1 + M07.3).
  2. User-defined inherent impls (`ImplRegistry.methods` — M07.4).
  3. **Trait impls** (NEW M07.6): iterate `TraitImplRegistry.impls` and look for an entry matching `(trait_name, receiver_ty.name())` whose `overrides` (or the trait's `default_methods`) contains the method.
  4. None → "no method `<name>` on type `<Ty>`".
- **Tie-breaker** (when type implements MULTIPLE traits both providing the method): not ambiguous at the dispatch level — the receiver's concrete type has its own impls; the dispatch picks the trait impl whose method matches. If the receiver is a Param with multiple bounds, THEN ambiguity error fires (R-010).

### R-012 — Default-method dispatch routes to trait body

- **Decision**: at dispatch time, lookup order for a trait method on a concrete type:
  1. `TraitImplRegistry.impls[(trait, type)].overrides[method]` — the impl's explicit body.
  2. Fall through to `TraitRegistry.schemas[trait].default_methods[method]` — the trait's default body.
- **Body execution**: the trait's default body executes with `self` bound to the receiver. Any `self.other()` calls inside re-dispatch through the standard machinery, picking up the impl's overrides for those methods.

### R-013 — Method-name ambiguity in multi-bound: error + UFCS hint

- **Decision**: when `fn foo<T: A + B>(x: T)` calls `x.name()` and both A and B declare `name`:
  - Error: `"ambiguous method `name` — candidates: `A::name`, `B::name`; use UFCS like `A::name(&x)` to disambiguate"`.
  - UFCS itself is OUT of scope, but the suggestion gives the learner a path forward.
- **Rationale**: explicit-error-with-hint is better than picking arbitrarily.

### R-014 — Inherent-wins-over-trait dispatch (extends M07.4 tie-breaker)

- **Decision**: when both an inherent impl and a trait impl define the same method name on the same type, the inherent impl wins. M07.4's existing tie-breaker pattern extends naturally: builtins > inherent > trait.
- **Rationale**: Rust standard; pedagogically clean ("inherent = the type's own methods").

## Eval

### R-015 — Trait-method dispatch frame name: `<Point as Show>::show`

- **Decision**: when a method call dispatches through a trait impl (not inherent, not builtin), the `FrameEnter.fn_name` uses Rust's UFCS-style format: `<Point as Show>::show`. This visually distinguishes trait dispatch from inherent (`Point::show`) — pedagogically helpful for showing which dispatch path was taken.
- **For default methods routed through the trait body**: same format — `<Point as Show>::double` (even though the body comes from the trait, the dispatch IS on Point's behalf).
- **For generic-fn-body dispatch through a bound** (`fn print<T: Show>(x: T) { x.show() }` called as `print(p)`): the inner trait-method frame is `<Point as Show>::show` (substituted concrete type; not `<T as Show>::show`).
- **Alternative considered**: simpler `Point::show` format. Rejected — conflates with inherent. The `<as>` distinction earns its visual weight.

### R-016 — Eval-side dispatch: which body to execute

- **Decision**: at eval's `eval_method_call`, after typeck has resolved which trait + method:
  1. Look up `TraitImplRegistry.impls[(trait, type)].overrides[method]` — if present, execute the impl's body.
  2. Else look up `TraitRegistry.schemas[trait].default_methods[method]` — execute the trait's default body.
  3. Self binding: same as M07.4's method-dispatch (build a `Value::Ref` to the receiver's slot for `&self` / `&mut self`; clone for `self`).
- **Trait + impl FnDecl lookup at eval time**: needs Evaluator-side parallel to typeck's registries. Add `trait_default_bodies: HashMap<(String, String), &'a ast::FnDecl>` and `trait_impl_bodies: HashMap<(String, String, String), &'a ast::FnDecl>` (keyed by `(trait, type, method)`) populated at Evaluator::new.

## Protocol

### R-017 — 10th invocation of the closed-enum-with-revisions rule

- **Decision**: amend M03's contract to note M07.6 as the 10th invocation. Changes:
  - **Additive variant on `Item`** (AST): `Item::Trait(TraitDecl)`. AST is not part of the wire protocol (it's parser-side); no JSON impact.
  - **AST extension**: `ImplBlock.trait_name: Option<String>` (None = inherent, Some = trait).
  - **AST extension**: `TypeParam.bound: Option<String>` (M07.5) → `bounds: Vec<String>` (M07.6).
  - **No event-side changes**. `FrameEnter.fn_name` carries the mangled `<Point as Show>::show` format — no protocol change, just a string convention.
  - **No new Ty/Value/Pointee variants**.
- **Precedent chain**: M03.1 → M03.2 → M06 → M07 → M07.1 → M07.2 → M07.3 → M07.4 → M07.5 → **M07.6**.
- **Snapshot byte-identity**: M03 stays byte-identical (no event-shape changes, no Ty/Value shape changes). M01/M02 may re-baseline once for `TypeParam.bound` → `bounds` Vec promotion (Debug output changes from `bound: None` to `bounds: []`). Programs that don't use traits stay byte-identical.
