# Data Model — M05 Entities

M05 introduces one new type (`CompileError`) and one new structured wire-format shape (`SetSourceResult`). Everything else is in-place modification.

## New: `CompileError`

```rust
// In src/pipeline.rs

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompileError {
    /// Source location of the error (byte offsets + FileId).
    pub span: Span,
    /// Which pipeline stage produced the error.
    pub stage: CompileStage,
    /// Human-readable error message.
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CompileStage {
    Parse,
    Resolve,
    Typeck,
    Eval,
}
```

### Validation rules

- **VR-1**: `CompileError` is constructed via `From` impls on each per-stage error type (`ParseError`, `resolve::Error`, `typeck::Error`, `eval::Error`). The `From` impl sets `stage` to the appropriate variant and copies `span` + `message`.
- **VR-2**: Pipeline runs short-circuit — the first failing stage produces a `CompileError` and no later stage runs. `stage` always reflects the actual failing stage.
- **VR-3**: `CompileStage` is closed for M05. Adding stages requires a coordinated update (same closed-enum-with-revisions rule as `MemEvent`).

## New: `SetSourceResult` (serde-only, wire-format)

The JSON shape returned by `Player::set_source(...)`. Not exposed as a named Rust struct — internal serialization is via an inline anonymous serde struct for clarity. JS sees it as one of:

```jsonc
// Success:
{
  "ok": true,
  "state": { /* full StateSnapshot */ }
}

// Error:
{
  "ok": false,
  "error": {
    "span":    { "start": 12, "end": 13, "file": 1 },
    "stage":   "Parse" | "Resolve" | "Typeck" | "Eval",
    "message": "expected expression after `=`"
  }
}
```

### Validation rules

- **VR-4**: Exactly one of `state` or `error` is present, gated by `ok`.
- **VR-5**: On error, `state` is absent. JS should not rely on a stale `state()` snapshot — it should call `state()` separately if it needs the empty cursor's state.
- **VR-6**: On success, `error` is absent. JS should clear any persisted error decoration when it sees `ok: true`.

## Modified: `Player` (in `src/ui.rs`)

```rust
#[wasm_bindgen]
pub struct Player {
    cursor: Cursor,
    source: String,
    /// M05: last error from a `set_source` call, if any. Cleared on the next
    /// successful re-run. Used as backing storage for `error_message()` etc.
    last_error: Option<CompileError>,
}

#[wasm_bindgen]
impl Player {
    /// M05: takes source code (not a trace JSON). Infallible; on parse/resolve/
    /// typeck/eval error, the Player exists with empty cursor + recorded error.
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str) -> Player { /* ... */ }

    /// M05: re-runs the pipeline on `source`. Returns JSON of shape
    /// `{ok: true, state: <StateSnapshot>} | {ok: false, error: <CompileError>}`.
    pub fn set_source(&mut self, source: &str) -> String { /* ... */ }

    /// Existing M04 methods unchanged (semantics extended where natural):
    pub fn state(&self) -> String { /* StateSnapshot JSON */ }
    pub fn source(&self) -> String { /* current source */ }
    pub fn step_forward(&mut self) -> String { /* state JSON */ }
    pub fn step_back(&mut self) -> String { /* state JSON */ }
    pub fn rewind(&mut self) -> String { /* state JSON */ }
    pub fn position(&self) -> usize { /* ... */ }
    pub fn total(&self) -> usize { /* ... */ }
}
```

### Validation rules

- **VR-7**: After a successful `set_source`, `self.cursor` is a fresh `Cursor::new(events)` at position 0; `self.source` is updated to the new source; `self.last_error` is `None`.
- **VR-8**: After a failing `set_source`, `self.cursor` is `Cursor::new(Vec::new())` (empty trace); `self.source` is updated to the new source (so `source()` still reflects what the user typed); `self.last_error` is `Some(CompileError)`.
- **VR-9**: `Player::new(source)` calls `set_source(source)` internally and discards the returned JSON. The Player exists either way.

## Modified: trace-file fetch flow (in `web/index.js`)

Previously `loadSample(id)` did:

```js
const res = await fetch(`/traces/${id}.json`);
const traceText = await res.text();
player = new Player(traceText);  // parsed TraceFile { source, events }
```

After M05:

```js
const res = await fetch(`/samples/${id}.rs`);
const source = await res.text();
editor.setValue(source);  // triggers updateListener → debounced set_source
```

### Validation rules

- **VR-10**: Sample fetches go to `/samples/<id>.rs` (not `/traces/...`). The `<link data-trunk rel="copy-dir" href="samples">` directive serves the directory.
- **VR-11**: `editor.setValue(source)` fires the same `updateListener` that user typing does. There's no separate "load sample" code path — sample-load and user-edit converge on the same debounced re-run.

## New: `tests/samples/m05_*.rs` + `web/samples/m05_*.rs`

| File                       | Notes                                          |
|----------------------------|------------------------------------------------|
| `m05_minimal.rs`           | `fn main() { let x = 5; }` — smallest L1.       |
| `m05_let_chain.rs`         | 3 sequential lets, each referencing the previous. |
| `m05_double.rs`            | `fn double(n)` + `main` calls it once.          |
| `m05_broken_parse.rs`      | `fn main() { let x = ; }` — deliberate error.   |

### Validation rules

- **VR-12**: The valid samples (`minimal`, `let_chain`, `double`) parse, resolve, type-check, and evaluate without error. Their traces contain at least one `ReturnValue` event and zero `SlotDrop` events (L1 is all Copy, per M03.1).
- **VR-13**: `m05_broken_parse.rs` deliberately fails the parse stage. `run_pipeline` returns `Err(CompileError { stage: Parse, ... })`. The error's `span` covers the `;` that follows the empty initializer (or whatever the parser chooses as the missing-expression site).
- **VR-14**: Both copies (`tests/samples/` and `web/samples/`) hold identical content. They're separate so the M03 integration tests don't depend on the web/ tree.

## Re-baselined artifacts

| Path                          | Change                                              |
|-------------------------------|-----------------------------------------------------|
| `tests/snapshots/`            | unchanged — M05 doesn't touch existing snapshots. |
| `web/traces/`                 | obsolete after M05; gitignored already. Optional cleanup. |

M01, M02, M03 (post-M03.1) snapshots untouched.
