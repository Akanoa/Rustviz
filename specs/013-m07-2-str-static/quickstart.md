# Quickstart — M07.2 development + verification

Audience: maintainer + contributors working on M07.2 or extending it.

## Run the page

```bash
cd web && trunk serve --open
```

After M07.2 ships, the dropdown gains 3 entries: `Str literal`, `String::from + static`, `push_str + static`. The page layout gains a new region labeled "static memory (RO)" between the stacks and heap panels.

## Run all tests

```bash
cargo test                            # full suite

cargo test --lib pipeline::tests::run_pipeline_str_literal      # &str literal
cargo test --lib pipeline::tests::run_pipeline_string_from      # M07 test, re-baselined: now checks both StaticAlloc + HeapAlloc
cargo test --lib pipeline::tests::run_pipeline_string_from_static_visible  # NEW: both blocks coexist
cargo test --lib pipeline::tests::run_pipeline_literal_dedup    # "hi"; "hi"; → one StaticAlloc
cargo test --lib pipeline::tests::run_pipeline_str_len          # s.len() on &str
cargo test --lib pipeline::tests::run_pipeline_push_str_static  # push_str's "!" comes from static
```

M01, M02, M03 should stay byte-identical (no existing L1 sample constructs string literals).

## Manual QA procedure (SC-008)

~8 minutes. Walk in this order:

1. **Page loads** with the default sample. No console errors. The new "static memory (RO)" region renders as an empty band between stacks and heap (no literals yet → no blocks).

2. **US1 — String literal as `&str` (the headline)**:
   - Select `Str literal (M07.2)`. Editor shows `let s = "toto";`.
   - Step. Observe at the `let s = "toto"` step:
     - **A static-region block appears** labeled `static #0 "toto" (4B)` with 4 byte-cells in gray (read-only color).
     - **A blue slice arrow** connects `s`'s slot to the static block, annotated with `[len: 4]`.
     - `s`'s row in the stacks panel shows `s : &str` (NOT `s : String`).
     - **No heap activity** — heap panel stays empty.
   - Hover the slice arrow → the static block's 4 byte-cells light up yellow. Pedagogy: "this slice views these 4 bytes in the binary's RO segment".

3. **US2 — `String::from` copies static to heap**:
   - Select `String::from + static (M07.2)`. Editor shows `let s = String::from("hi");`.
   - Step through:
     1. `String::from("hi")` call: static block `"hi"` appears in static region.
     2. Heap allocation fires: heap block `String "hi" (cap=2)` appears with black owning arrow from `s`.
   - Observe both blocks side-by-side, both showing `"hi"` bytes — the copy is explicit.
   - Step to end-of-`main`: `HeapFree` fires for the heap String (heap block disappears); the static block STAYS visible.

4. **US3 — `push_str` with static literal**:
   - Select `push_str + static (M07.2)`. Editor shows `let mut s = String::from("hi"); s.push_str("!");`.
   - Step. Observe:
     1. Static region gains `"hi"` block.
     2. Heap gains String block `"hi"` (cap=2).
     3. `push_str("!")` call: static region gains a second block `"!"` (1 byte).
     4. Heap String's bytes update to `"hi!"` (may realloc if cap exceeded — depends on growth policy; with cap=2 going to 3 bytes, realloc fires).
   - Two static blocks visible; one heap block (potentially re-addressed if realloc).

5. **Literal dedup**:
   - Type `fn main() { let a = "hi"; let b = "hi"; }`. Observe: static region has ONE `"hi"` block (NOT two). Two blue slice arrows (a → static, b → static) both pointing at the same block. Pedagogy: "Rust's linker merges duplicate string constants".

6. **`s.len()` on `&str`**:
   - Type `fn main() { let s = "toto"; let n = s.len(); }`. Observe `n : u64 = 4_u64` in the stacks panel.

7. **Regressions**:
   - Cycle through M01–M07.1 samples. Each renders correctly. M07's `String (M07)` sample now shows the static `"hi"` and `"!"` blocks in addition to the heap String — slightly richer than M07's behavior, but the heap pedagogy is unchanged.

## Developer notes

### Why `Ty::Str` instead of `Ty::Slice(Box::new(Ty::Int(U8)))`?

Pedagogical clarity. Rust developers expect `let s = "hi"` to give `s : &str`, not `s : &[u8]`. Internally the two are equivalent (M07.2's typeck treats them interchangeably for method dispatch, borrow tracking, aliasing); the distinction is purely in `Ty::name()`'s rendering. One variant is simpler than peephole-rendering every `Ty::Slice(Box::new(Ty::Int(U8)))` site.

### Why dedupe by content?

Matches Rust's actual linker behavior — `.rodata` merges duplicate string constants. Two `"hi"` literals in real Rust share the same address. The pedagogy aligns: "the bytes live in the binary; if two places need the same constant, they point at the same bytes".

### Why is `Value::Str` removed?

M07 used `Value::Str(String)` as a transient — an internal Rust-side value carrying the literal's bytes for `String::from`'s arg-extraction path. With M07.2, literals become `Value::Slice { target: Pointee::Static(_), .. }` from the start; `String::from` extracts bytes via the static-region lookup. Nothing constructs `Value::Str` anymore — keeping it would be dead code.

### Static region position + styling

The new `<section id="static">` sits between `#stacks` and `#heap` in the page grid. Default styling: subtle gray gradient background, "static memory (RO)" italic label at the top, byte-cells in muted gray (`#bbb` filled vs `#fff` for empty, vs heap's blue). The block carries the literal's content in quotes for readability (`"toto"`).

### Arrow routing into static

Static blocks are likely above heap (depending on final layout). The renderer's "enter from above" routing for heap targets generalizes — static targets use the same path (route up + over + down to the block's top edge), just landing on a different region. Per-arrow lane stagger (M07/M07.1) still applies.

### M07 `m07_string` test re-baseline

The existing `run_pipeline_string_from` test currently does:

```rust
assert_eq!(alloc_count, 1);
```

After M07.2 the trace gains a `StaticAlloc` for `"hi"`. Update the assertion to count alloc events by variant:

```rust
let heap_count = events.iter().filter(|e| matches!(e, MemEvent::HeapAlloc { .. })).count();
let static_count = events.iter().filter(|e| matches!(e, MemEvent::StaticAlloc { .. })).count();
assert_eq!(heap_count, 1, "expected one heap allocation for the String buffer");
assert_eq!(static_count, 1, "expected one static block for the literal");
```

### Removed `Value::Str` cascade

Match-exhaustiveness will flag every `Value::Str` arm. Sites known in advance:
- `event.rs::Value::type_name()` — drop the `Value::Str` arm
- `eval.rs::value_size_bytes()` — drop the `Value::Str` arm
- `eval.rs::render_value_for_note()` — drop the `Value::Str` arm
- `ui.rs::render_value()` — drop the `Value::Str` arm
- `eval.rs::eval_path_call(["String", "from"])` — replace `match self.eval_expr(&args[0]) { Value::Str(s) => s, ... }` with `Value::Slice { target: Pointee::Static(addr), .. } => extract_static_bytes(addr)`. The `eval_method_call(["push_str"])` arm similarly.

## When extending in M08

M08 (threads) introduces `Arc<T>` and `Mutex<T>` — both heap-allocated. The static region is unaffected; threads don't have thread-local static. No additional protocol changes needed for M08 from M07.2's side.

## What this milestone does NOT add

- `&str` slicing (`&s[1..3]` where `s: &str`) — deferred.
- `&str` indexing (`s[0]`) — Rust rejects, M07.2 follows.
- `format!`, `println!`, `write!`, `print!` — out of scope.
- String concat operators (`+`, `+=`) — out of scope.
- Generalized `&str` args in `push_str` / `String::from` — args must still be literal expressions.
- UTF-8 character-level pedagogy (`char` boundaries, code-point counting) — static blocks render raw bytes.
- Static items (`static FOO: i32 = 5;`) — only string literals get static blocks.
- `&'a str` lifetimes beyond `'static` — out of scope.
