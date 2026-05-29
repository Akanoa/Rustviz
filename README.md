# rustviz

**A pedagogical visualizer for Rust ownership, borrowing, and concurrency** — runs in the browser via WebAssembly. Type Rust, press a step button, watch the stacks, heap, vtables, and threads play forward.

The aim is to make tangible what is usually abstract:

- *"this binding is moved, so it's grayed out now"*
- *"`&v[0]` becomes a dangling reference after `v.push()`"*
- *"`Arc::clone` shares the heap allocation — the refcount bumps from 1 to 2"*
- *"the closure captured `m2` by move; the source slot is still on main's frame, just unusable"*

---

## Try it locally

Requires Rust ≥ 1.85, the `wasm32-unknown-unknown` target, and [Trunk](https://github.com/trunk-rs/trunk).

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk

cd web
trunk serve --open
```

The dropdown holds ~50 sample programs covering every milestone (M01 lexer/parser through M08 concurrency). Each sample demonstrates a specific Rust mechanism.

---

## What's visualized

Three strictly decoupled layers — interpreter emits events, UI replays them with a cursor.

```
┌─────────────────────────────────────────────────┐
│  UI (web, WASM bindings)                        │
│  Editor │ Stacks (per-thread) │ Heap │ Vtables  │
└────────────┬────────────────────────────────────┘
             │ replays
┌────────────┴────────────────────────────────────┐
│  Event stream (Vec<MemEvent>)                   │  ← single source of truth
└────────────┬────────────────────────────────────┘
             │ emits
┌────────────┴────────────────────────────────────┐
│  Interpreter (Rust → WASM)                      │
│  parse → resolve → typeck → eval                │
└─────────────────────────────────────────────────┘
```

### Memory regions

| Region | Visualization |
|---|---|
| **Stacks** | One column per live thread. Each column is a stack of frame cards; each card holds slots (`name : type = value`). Innermost frame on top. Returned frames stay grayed-out — the bytes physically persist until the storage is reused. |
| **Heap** | One block per live allocation. Box / Vec / String / Arc all visible; `Arc<T>` blocks show `[refs: N]` refcount; `Mutex<T>` blocks show a green `🔓 free` / red `🔒 by #N` lock badge; freed blocks gray out (memory persists until reused). |
| **Vtables** | One box per `(trait, type)` pair, content-deduplicated like the linker would. M07.7+. |
| **Static memory (RO)** | One block per unique `&'static str` literal, content-deduplicated. M07.2+. |

### Arrow overlay

All arrows are hover-only by default — hover a slot to reveal its outgoing arrow.

| Color / style | Meaning |
|---|---|
| Solid blue | `&T` shared borrow |
| Solid red | `&mut T` mutable borrow |
| Solid black | Owning pointer (`Box<T>` / `Vec<T>` / `String`) |
| Dashed orange | Trait-object dispatch (transient — visible at the call step only) |
| Dashed purple | `Arc<T>` shared ownership |
| Dotted orange | Byte copy (`String::from` / `push_str`) — transient |

### Cursor model

The interpreter emits a `Vec<MemEvent>`; the UI replays a prefix of it. Step / step-back / rewind / play. Multi-event "atomic groups" (e.g. `SlotAlloc + SlotWrite`, `ArcClone + HeapRealloc + Note`) coalesce into one user-facing step so the counter advances by logical change, not by raw event.

---

## Rust subset

Staged across the milestone roadmap (see `MILESTONES.md` for the full list). Currently visualized:

| Level | Mechanisms |
|---|---|
| **L1** | primitives, `let` / `let mut`, functions, scopes, moves of non-Copy types, blocks as expressions, `if` expressions, operators with precedence |
| **L2** | `&T` / `&mut T`, aliasing rules, scope-level lifetimes, `*expr` deref, through-ref assignment (`*r = v`) |
| **L3** | `Box`, `Vec` (with visible realloc + dangling-reference detection), `String`, `&[T]` slices, `&str`, `[T; N]` arrays |
| **L4** | structs + impl methods, generics + monomorphization, traits + static dispatch, trait objects + vtables + dynamic dispatch, `thread::spawn` + `move ‖`, `Arc` + refcount, `Mutex` + lock state, `Arc<Mutex<T>>` |

Deliberately faithful where it helps pedagogy, deliberately simplified where it doesn't:

- `Arc<Mutex<T>>` fuses into one heap block (matches Rust's actual inline layout — `Arc<T>` stores `T` inline, so `Arc<Mutex<T>>` is one allocation).
- Moved bindings stay on the source frame card with grayed-out `<moved>` annotation (matches Rust: the stack bytes physically persist; only the type system stops you from referencing them).
- Mutex contention parking is not yet visualized (M08 v1 limitation — the cooperative scheduler runs spawned threads inline at the next yield point; planned for M08.1).
- `Send` / `Sync` auto-trait inference is deliberately out of scope — full inference is rustc-grade work.

---

## Repository layout

```
src/
├── parse/        # lexer, AST, recursive-descent parser
├── resolve.rs    # name resolution (Ident → BindingId)
├── typeck.rs     # type checking + trait/impl/dyn dispatch
├── eval.rs       # AST-walking evaluator — emits Vec<MemEvent>
├── event.rs      # the MemEvent enum (closed, with revision history)
├── ui.rs         # cursor + state-snapshot computation + WASM bindings
└── pipeline.rs   # top-level: source → events; integration tests live here

tests/
├── m01.rs / m02.rs / m03.rs   # milestone snapshot tests via insta
└── samples/                   # ~50 reference programs (M01 → M08)

web/
├── index.html  index.js  style.css   # UI shell (Monaco-free; plain DOM)
├── samples/                          # browser-side mirrors of tests/samples
└── Trunk.toml

specs/                  # one folder per feature/milestone — speckit workflow
MILESTONES.md           # roadmap, scope per milestone, exit criteria
CLAUDE.md               # contributor architecture notes
```

The architectural rule: **the interpreter never writes to the UI directly**. It emits a typed event stream. The UI replays that stream with a cursor. Step-by-step and rewind become trivial; reordering and coalescing become a UI concern.

---

## Building & testing

```sh
# Native test suite — interpreter snapshots + pipeline integration tests.
cargo test

# Release-mode WASM bundle.
cd web && trunk build --release

# Dev mode with live reload on Rust + JS + CSS edits.
cd web && trunk serve
```

The release WASM bundle is ~440 KB (post-staged, pre-wasm-opt) as of M08 v1.

---

## Project conventions

- **Pedagogical first**: readability over optimization. Comment the *why* of subtle choices, not the *what*.
- **Faithfulness to `rustc` is not a goal**. If a simplification helps the visualization without misleading the reader, take it.
- **Closed-enum protocol with revisions**: `MemEvent` / `Ty` / `Value` are closed enums. Adding a variant is a "revision milestone" (e.g. M03.1, M07.7, M08) with a documented protocol delta in `specs/<feature>/contracts/`.
- **No parser framework**: hand-rolled recursive descent. Decision recorded in `CLAUDE.md`.

Contributions and bug reports welcome via GitHub issues.

---

## License

Dual-licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

at your option.
