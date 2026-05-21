# Contract — M05 Player API + Sample Loading

M05 extends the M04 `Player` contract (`specs/005-m04-ui-shell/contracts/m04-api.md`) with one new method and updates one existing method's semantics. Sample loading switches from JSON-trace fetch to source-file fetch.

## Player API additions

### `Player::new(source: &str) -> Player` *(semantics changed from M04)*

**M04 semantics**:

```rust
pub fn new(trace_json: &str) -> Result<Player, JsValue>
```

Took a pre-recorded trace JSON document with `{source, events}` shape. Returned `Err` if JSON parsing failed.

**M05 semantics**:

```rust
#[wasm_bindgen(constructor)]
pub fn new(source: &str) -> Player
```

Takes raw Rust source. Internally calls `set_source(source)`. **Infallible** — on parse/resolve/typeck/eval error, the Player is created with an empty cursor and a recorded `last_error`.

JS callers must update the call site:

```diff
- const player = new Player(traceJsonText);
+ const player = new Player(sourceText);
```

This is a breaking change to the M04 `Player::new` contract. Per the M04 contract's additive-revisions rule (lifted in M03.1's relaxation), removing or changing existing method signatures requires maintainer consent. M05 invokes this exception for the new signature.

### `Player::set_source(&mut self, source: &str) -> String` *(new in M05)*

```rust
#[wasm_bindgen]
pub fn set_source(&mut self, source: &str) -> String
```

Runs the M01 → M02 → M03 pipeline on `source`. Returns JSON.

**Success shape**:

```jsonc
{
  "ok": true,
  "state": {
    "frames": [...],
    "editor_highlight": null,
    "current_call_span": null,
    "status": null,
    "pending_return": null,
    "position": 0,
    "total": <event_count>
  }
}
```

After success: `self.cursor` is a fresh `Cursor::new(events)`; `self.source = source.to_owned()`; `self.last_error = None`.

**Error shape**:

```jsonc
{
  "ok": false,
  "error": {
    "span": { "start": <usize>, "end": <usize>, "file": <u32> },
    "stage": "Parse" | "Resolve" | "Typeck" | "Eval",
    "message": "<human-readable>"
  }
}
```

After error: `self.cursor` is `Cursor::new(Vec::new())` (empty); `self.source = source.to_owned()`; `self.last_error = Some(CompileError { ... })`.

The cursor is always reset to position 0 on `set_source` (whether the call succeeded or failed).

## Decoration extensions in M04's editor surface

The M04 contract's CodeMirror decoration list grows one entry:

| Decoration              | Class           | Source field                | M05 |
|-------------------------|-----------------|-----------------------------|-----|
| Yellow event-span       | `cm-current-span` | `state.editor_highlight`    | -   |
| Red call-site border    | `cm-current-fn`   | `state.current_call_span`   | -   |
| **Red wavy error span** | `cm-error-span`   | last `set_source` error's `span` | **NEW** |

The error decoration is **independent** of any `StateSnapshot` field — it's driven by the most recent `set_source` result. JS owns the decoration lifecycle: paint on `set_source` Err, clear on `set_source` Ok.

## Sample loading endpoint change

| Aspect       | M04                                         | M05                                          |
|--------------|---------------------------------------------|----------------------------------------------|
| HTTP path    | `/traces/<id>.json`                         | `/samples/<id>.rs`                           |
| Content      | `{source, events: MemEvent[]}` JSON         | raw Rust source text                         |
| Sets         | `player = new Player(traceJson)`            | `editor.setValue(source)` → debounced → `player.set_source(source)` |
| Side effect  | Resets cursor; updates editor source        | Identical end-state via the update listener  |

Trunk directive update in `index.html`:

```diff
- <link data-trunk rel="copy-dir" href="traces" />
+ <link data-trunk rel="copy-dir" href="samples" />
```

The old `traces/` copy-dir is removed (page no longer fetches them). The `samples/` copy-dir is added so `/samples/<id>.rs` resolves.

## Behavioral guarantees

- **B-M5-1**: `Player::set_source(s)` is idempotent: calling it twice with the same `s` yields the same `state`/`error`.
- **B-M5-2**: After a `set_source` Ok, `Player::state()` returns the same JSON as `set_source`'s `state` field.
- **B-M5-3**: After a `set_source` Err, `Player::state()` returns a state with `frames: []`, `position: 0`, `total: 0`. (The empty trace's state-at-0.)
- **B-M5-4**: `Player::step_forward()` and `step_back()` after an Err are no-ops because the trace is empty.
- **B-M5-5**: The same `CompileError` shape is returned whether the source has a parse, resolve, typeck, or eval error — distinguished only by `stage`. JS consumers display it uniformly.
- **B-M5-6**: The HTTP path `/traces/*.json` may still be served (gitignored leftover files) but the page never requests it. Removing those files is a no-op.

## What this contract does NOT cover (deferred)

- **Multiple-error display**: M01–M03 stop at the first error. M05 doesn't add multi-diagnostic support.
- **Editor lint gutter / problem panel**: a future polish milestone could add `@codemirror/lint` for a more conventional IDE-style errors view. M05 sticks with status-bar + wavy underline.
- **Autosaved drafts / local storage**: the editor's content is ephemeral. Refreshing the page reloads the default sample.
- **Code formatting / auto-indent on enter**: the default CodeMirror behavior applies. Tab inserts indentation (already landed pre-plan); no `rustfmt` integration.
- **Live evaluation as you type without debounce**: deliberately debounced. A "run immediately on every keystroke" mode could be added but isn't M05.
