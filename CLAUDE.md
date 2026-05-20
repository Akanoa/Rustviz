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

## Recent Changes
- 001-milestone-roadmap: Added N/A — deliverable is markdown documentation + `CLAUDE.md` (authoritative scope source); `specs/001-milestone-roadmap/spec.md` (this feature's spec)
