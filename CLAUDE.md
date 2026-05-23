# rustviz

Pedagogical visualizer for Rust ownership and borrowing, aimed at beginners. Web app (WASM) with three panels — code editor, stacks (multi-thread), and heap with animated pointers.

## Pedagogical goal

Give a newcomer concrete intuition for Rust's memory mechanics: moves, borrows, lifetimes, drops, heap allocations, threads with `Arc`/`Mutex`. The visualization should make tangible what is usually abstract — "this binding is moved, so it's unusable now", "this `&v[0]` becomes UB after `v.push()`", etc.

## Architecture

Three strictly decoupled layers:

```
┌─────────────────────────────────────────────┐
│  UI (web, WASM bindings)                    │
│  Editor panel │ Stacks panel │ Heap panel   │
└──────────────┬──────────────────────────────┘
               │ replays
┌──────────────┴──────────────────────────────┐
│  Event stream (MemEvent[])                  │  ← single source of truth
└──────────────┬──────────────────────────────┘
               │ emits
┌──────────────┴──────────────────────────────┐
│  Interpreter (Rust → WASM)                  │
│  parser → AST → resolver → typeck → eval    │
└─────────────────────────────────────────────┘
```

**Core principle**: the interpreter never writes to the UI directly. It emits a typed event stream. The UI replays the stream with a cursor (play / pause / step / rewind). Step-by-step and rewind become trivial.

## Event model

`MemEvent` is the centerpiece. Categories:

- **Threads**: `ThreadSpawn`, `ThreadJoin`, `ThreadPark`
- **Frames**: `FrameEnter`, `FrameLeave`
- **Stack slots (bindings)**: `SlotAlloc`, `SlotWrite`, `SlotMove`, `SlotDrop`
- **Heap**: `HeapAlloc`, `HeapRealloc`, `HeapFree`
- **Borrows**: `BorrowShared`, `BorrowMut`, `BorrowEnd` (with `BorrowId` to materialize the borrow's lifetime visually)
- **Synchronization**: `LockAcquire`, `LockRelease`, `ArcClone`, `ArcDrop`
- **Pedagogy**: `Note { kind, message, span }`

Every event carries a `SourceSpan`. At step N, the UI highlights the current span in the editor AND emphasizes the impacted slots/cells — this is what produces the "wow" moments.

`Pointee` is an enum `Slot(SlotId) | Heap(HeapAddr)` — a `&T` can point into the stack or the heap.

`SlotMove` is intentionally distinct from `SlotDrop`: for a beginner, the moment a binding becomes unusable after a move is exactly what needs to be animated.

## The three panels

- **Editor** (Monaco or CodeMirror): decorator highlighting the span of the current event. Optional gutter marking already-executed lines.
- **Stacks**: one column per thread, each column a stack of "frame cards" containing slots (name, type, value or `<moved>`). Spawning a thread slides a new column in from the right. A thread parked on a mutex greys out and draws a dotted line to the slot holding the mutex.
- **Heap**: free-form area where each `HeapAlloc` creates a box (size ∝ `size`, label = type). `HeapRealloc` animates: the box moves and every arrow pointing to it follows. This is what makes `&v[0]`-after-`push` viscerally obvious.
- **Pointers**: SVG overlay across the panels. Colors: black = owning (`Box`, `Vec`, `String`), blue = `&`, red = `&mut`, dashed purple = `Arc`/`Rc`.

## Supported Rust subset (by levels)

Deliberately staged — each level introduces one new memory mechanism to visualize.

- **Level 1**: primitives, `let`/`let mut`, functions, scopes, moves of non-Copy types, blocks as expressions, `if` expressions, operators with precedence. No references yet.
- **Level 2**: `&` and `&mut`, aliasing rules, scope-level lifetimes.
- **Level 3**: `Box`, `Vec` (with visible realloc), `String`.
- **Level 4**: `thread::spawn`, `Arc`, `Mutex`, `Send`/`Sync`.

## Planned code layout

```
src/
  parse/
    span.rs     # Span, Spanned<T>, SourceMap (byte offsets + FileId)
    lexer.rs    # &str → Vec<Token>
    ast.rs      # AST types, spans at every level
    parser.rs   # recursive descent, Vec<Token> → Program
  resolve/      # next — Ident → BindingId, scope checks
  typeck/       # next — annotation checks, type propagation
  eval/         # next — AST walker, emits MemEvent
  event.rs      # next — MemEvent enum
```

## Locked-in decisions

- **No parser framework** (Elyze was evaluated and rejected): no native span tracking, no operator precedence, error messages too thin for a pedagogical tool. Hand-rolled recursive descent instead.
- **Separate lexer** (vs char-by-char): simplifies whitespace/comment handling and multi-char lookahead (`==`, `!=`, `<=`).
- **Spans = byte offsets + `FileId`**: multi-file ready from day one. Line/column computed on demand for error reporting.
- **Stop at first parse error**: enough for a live editor; can be relaxed later.
- **`Vec<Token>` instead of an iterator**: arbitrary `peek` distance, memory cost is negligible.
- **Reject `&` at the lexer in level 1**: clearer error than a vague parser explosion. Replace with `Amp`/`AmpMut` tokens when level 2 lands.

## Immediate roadmap

1. Integrate the `parse/` skeleton (span, lexer, ast, parser) — code already drafted in `conversation.html`.
2. Name resolver: `Ident` → `BindingId`, "use of undeclared variable" errors.
3. Lightweight typeck: validate annotations, propagate obvious types.
4. Define `MemEvent` and write the level-1 evaluator.
5. First UI prototype: single stack panel, static replay of a pre-recorded trace.

## Notes for Claude

- **Pedagogical first**: prefer readability over optimization. Comment the *why* of subtle choices.
- **Faithfulness to `rustc` is not a goal**. If a simplification helps the visualization without misleading the reader, take it.
- Code is in English; user-facing strings (errors, notes) can be either — decide per audience.

## Active Technologies
- N/A — deliverable is markdown documentation + `CLAUDE.md` (authoritative scope source); `specs/001-milestone-roadmap/spec.md` (this feature's spec) (001-milestone-roadmap)
- filesystem, version-controlled in git (001-milestone-roadmap)
- Rust 2024 edition (latest stable), MSRV pinned to current stable at scaffold time (recorded in `Cargo.toml`) + `insta` (snapshot testing). No parser framework (CLAUDE.md locked-in decision). No `thiserror`/`anyhow` for M01 — error type is a single hand-rolled struct. (002-m01-frontend-skeleton)
- N/A (in-memory only; SourceMap holds source text) (002-m01-frontend-skeleton)
- Rust 2024 edition, same toolchain as M01 (1.85+). No `Cargo.toml` changes other than registering the new `[[test]]` target `m02`. + existing `insta` dev-dep; no new deps. (003-m02-resolve-typeck)
- in-memory; metadata stored in `BTreeMap<Span, ...>` side tables for determinism. (003-m02-resolve-typeck)
- Rust 2024 edition, same toolchain as M01/M02. No `Cargo.toml` changes other than registering the new `[[test]]` target `m03`. + existing `indexmap` regular dep (used in M02), existing `insta` dev-dep. No new deps. (004-m03-event-eval)
- in-memory; the event stream is a `Vec<MemEvent>` accumulated as the evaluator walks the AST. (004-m03-event-eval)
- Rust 2024 edition (same toolchain as M01–M03). `wasm32-unknown-unknown` target for the WASM build. Modern browser JS (ES modules, fetch, async/await). + existing `indexmap` regular dep; NEW regular deps `serde` + `serde_json` (trace JSON serde) and `wasm-bindgen` + `js-sys` + `console_error_panic_hook` (WASM bindings + dev ergonomics). All WASM-portable and standard. (005-m04-ui-shell)
- pre-recorded traces are static JSON assets under `web/traces/` (gitignored, regenerated by `cargo run --bin gen_traces`). (005-m04-ui-shell)
- Rust 2024 edition (same toolchain as M03/M04). No new toolchain requirements. + existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new deps** — this is a protocol revision. (006-m03-1-protocol-revision)
- in-memory; trace JSON shape gains the new `ReturnValue` variant case and loses the `FrameEnter.params` field. (006-m03-1-protocol-revision)
- Rust 2024 edition (same toolchain as M01–M03.1). No new toolchain requirements. + existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. New JS dependency on `@codemirror/commands@6` (already added pre-plan for the Tab-handling adjustment; reused here for the keymap). (007-live-l1-editing)
- in-memory; no new files. `web/traces/*.json` files become deprecated artifacts (FR-010); trunk's pre-build `gen_traces` hook is dropped. (007-live-l1-editing)
- Rust 2024 edition (same toolchain as M01–M05). No new toolchain requirements. + existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. No JS dep changes. (008-m03-2-scalar-lattice)
- in-memory; no new files. M03 snapshot tests re-baselined (Value's Debug format changes); `web/traces/` remains obsolete (M05 already removed the trunk hook). (008-m03-2-scalar-lattice)
- Rust 2024 edition (same toolchain as M01–M05). No new toolchain requirements. + existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. No JS dep changes (existing `@codemirror/*` import map sufficient). (009-m06-borrows)
- in-memory; no new files. M03 snapshot tests should stay byte-identical (existing samples don't construct `Value::Ref` or `Ty::Ref`). M02 may re-baseline if any TypeMap snapshot Debug output shifts (unlikely — additive enum variants don't change existing variant formats). (009-m06-borrows)
- Rust 2024 edition (same toolchain as M01–M07). No new toolchain requirements. + existing `indexmap`, `serde`, `serde_json`, `wasm-bindgen`, `js-sys`, `console_error_panic_hook`. **No new Rust deps**. **No JS deps changes**. (012-m07-1-slices)
- in-memory; no new files. M01/M02/M03 snapshot tests should stay byte-identical (existing samples don't construct `Value::Slice`, and `Ty::Slice` / `Value::Slice` are additive variants). (012-m07-1-slices)

## Recent Changes
- 001-milestone-roadmap: Added N/A — deliverable is markdown documentation + `CLAUDE.md` (authoritative scope source); `specs/001-milestone-roadmap/spec.md` (this feature's spec)
