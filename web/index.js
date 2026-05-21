// M04 — vanilla JS driver. Wires CodeMirror + WASM Player + DOM.
//
// Architecture: all logic lives in the WASM Player (rustviz::ui). This file
// just (a) loads the trace JSON, (b) renders the StateSnapshot, and
// (c) handles control events.

// CodeMirror imports go through the import map in index.html — `?external=`
// URLs emit bare specifiers which the map resolves to one canonical URL,
// ensuring all CM packages share the same @codemirror/state / view / language
// instances (otherwise instanceof checks fail across modules).
//
// We deliberately skip `basicSetup` from the `codemirror` meta package: it
// pulls in 6+ extra sub-packages (commands, search, autocompletion, lint, ...)
// that we don't need for read-only display and that complicate the import map.
// A minimal assembly of `lineNumbers` + `rust` + `syntaxHighlighting` is enough.
import { EditorState, StateEffect, StateField } from "@codemirror/state";
import { EditorView, Decoration, lineNumbers, keymap } from "@codemirror/view";
import { syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";
import { rust } from "@codemirror/lang-rust";
import { indentWithTab } from "@codemirror/commands";

// Trunk's `<link data-trunk rel="rust" data-type="main">` auto-injects a
// script that runs the WASM init, then dispatches a `TrunkApplicationStarted`
// event with the bindings on `window.wasmBindings`. We hook that event below.
let Player = null;

const PLAY_RATE_MS = 400;

const SAMPLES = [
  { id: "m03_arithmetic", label: "Arithmetic" },
  { id: "m03_fn_call", label: "Function Call" },
  { id: "m03_shadow", label: "Shadowing" },
  { id: "m03_div_by_zero", label: "Division by Zero" },
];

// ─── CodeMirror highlight states ──────────────────────────────────────────
// Two layers of decoration:
//   1. `highlightField`  — yellow background on the most-recent event's span
//   2. `currentFnField`  — red left border on the currently-executing fn's body
// Both are mark decorations; CodeMirror composes them transparently.

const setHighlight = StateEffect.define(); // payload: { start, end } | null
const highlightField = StateField.define({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (e.is(setHighlight)) {
        if (e.value === null) {
          deco = Decoration.none;
        } else {
          const { start, end } = e.value;
          deco = Decoration.set([
            Decoration.mark({ class: "cm-current-span" }).range(start, end),
          ]);
        }
      }
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

// M03.1: red border around the currently-executing function's body span.
const setCurrentFn = StateEffect.define(); // payload: { start, end } | null
const currentFnField = StateField.define({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (e.is(setCurrentFn)) {
        if (e.value === null) {
          deco = Decoration.none;
        } else {
          const { start, end } = e.value;
          deco = Decoration.set([
            Decoration.mark({ class: "cm-current-fn" }).range(start, end),
          ]);
        }
      }
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

// M05 / US2: red wavy underline at a compile-error span. Cleared on success.
const setError = StateEffect.define(); // payload: { start, end } | null
const errorField = StateField.define({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (e.is(setError)) {
        if (e.value === null) {
          deco = Decoration.none;
        } else {
          const { start, end } = e.value;
          // CodeMirror requires from < to; clamp degenerate spans to a 1-char range.
          const safeEnd = end > start ? end : start + 1;
          deco = Decoration.set([
            Decoration.mark({ class: "cm-error-span" }).range(start, safeEnd),
          ]);
        }
      }
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

// ─── State + globals ──────────────────────────────────────────────────────

let editorView = null;
let player = null;
let playInterval = null;
let debounceTimer = null;

const DEBOUNCE_MS = 300;

// ─── DOM helpers ──────────────────────────────────────────────────────────

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else node.setAttribute(k, v);
  }
  for (const c of children) {
    if (c != null) node.appendChild(c);
  }
  return node;
}

// ─── render(state) — apply a StateSnapshot to the DOM ─────────────────────

function render(state) {
  // Stacks panel: rebuild from scratch.
  const stacksEl = document.getElementById("stacks");
  stacksEl.replaceChildren();
  for (const frame of state.frames) {
    // M03.1 styling states (mutually-exclusive for grayed/current):
    //   • `frame-grayed`: frame has returned (active === false); slots area
    //     at reduced opacity, name muted with strikethrough.
    //   • `frame-current`: innermost active frame — the one whose body is
    //     currently executing. Distinguishes callee (executing) from caller
    //     (paused).
    const cls = ["frame-card"];
    if (!frame.active) cls.push("frame-grayed");
    if (frame.current) cls.push("frame-current");
    const card = el("div", { class: cls.join(" ") });
    card.setAttribute("data-frame-id", String(frame.frame_id));
    const header = el("div", { class: "frame-header" });
    header.appendChild(el("span", { class: "frame-name", text: `${frame.fn_name}()` }));
    // M03.1: `frame.return_value` is set once a `ReturnValue` event has fired
    // for this frame and persists across the subsequent `FrameLeave` — so
    // the `→ <value>` annotation stays visible on the grayed frame, not just
    // on the single ReturnValue tick.
    if (frame.return_value !== null && frame.return_value !== undefined) {
      header.appendChild(
        el("span", {
          class: "frame-return-value",
          text: `→ ${frame.return_value}`,
        }),
      );
    }
    card.appendChild(header);
    const slotGrid = el("div", { class: "slots" });
    for (const slot of frame.slots) {
      const row = el("div", { class: "slot-row" });
      row.appendChild(el("span", { class: "slot-name", text: slot.name }));
      row.appendChild(el("span", { class: "slot-ty", text: `: ${slot.ty}` }));
      const valueEl = el("span", { class: "slot-value" });
      if (slot.value === null || slot.value === undefined) {
        valueEl.classList.add("slot-pending");
      } else {
        valueEl.textContent = `= ${slot.value}`;
      }
      row.appendChild(valueEl);
      slotGrid.appendChild(row);
    }
    card.appendChild(slotGrid);
    stacksEl.appendChild(card);
  }

  // Editor span highlight (yellow, most-recent event).
  if (state.editor_highlight) {
    const { start, end } = state.editor_highlight;
    editorView.dispatch({ effects: setHighlight.of({ start, end }) });
  } else {
    editorView.dispatch({ effects: setHighlight.of(null) });
  }

  // M03.1: red border around the currently-executing function's body span.
  if (state.current_call_span) {
    const { start, end } = state.current_call_span;
    editorView.dispatch({ effects: setCurrentFn.of({ start, end }) });
  } else {
    editorView.dispatch({ effects: setCurrentFn.of(null) });
  }

  // Status message.
  const statusEl = document.getElementById("status");
  if (state.status) {
    statusEl.hidden = false;
    statusEl.className = state.status.kind === "error" ? "status-error" : "status-info";
    statusEl.textContent = state.status.message;
  } else {
    statusEl.hidden = true;
    statusEl.textContent = "";
    statusEl.className = "";
  }

  // Step indicator.
  document.getElementById("step-indicator").textContent = `${state.position} / ${state.total}`;

  // M05 / US2: success path — clear any error underline + re-enable controls.
  editorView.dispatch({ effects: setError.of(null) });
  setControlsEnabled(true);
}

// M05 / US2: render a compile error. Underline the span, show the message
// in the status bar, disable playback controls.
function renderError(error) {
  // Editor underline at the error span.
  editorView.dispatch({ effects: setError.of({ start: error.span.start, end: error.span.end }) });

  // Status bar: prefix with the stage so the user sees "Parse error: ...".
  const statusEl = document.getElementById("status");
  statusEl.hidden = false;
  statusEl.className = "status-error";
  statusEl.textContent = `${error.stage} error: ${error.message}`;

  // Frames panel is empty (set_source replaced cursor with empty trace).
  document.getElementById("stacks").replaceChildren();
  document.getElementById("step-indicator").textContent = "0 / 0";

  // Editor decorations from prior successful runs no longer apply.
  editorView.dispatch({
    effects: [setHighlight.of(null), setCurrentFn.of(null)],
  });

  setControlsEnabled(false);
}

// M05 / US2: toggle Play / Step Forward / Step Back disabled state. Rewind
// stays always enabled because rewinding an empty trace is a meaningful
// no-op.
function setControlsEnabled(enabled) {
  for (const id of ["btn-play-pause", "btn-step-back", "btn-step-forward"]) {
    document.getElementById(id).disabled = !enabled;
  }
}

// ─── M05: live pipeline + sample loading ──────────────────────────────────

// US1: replace the editor source. The editor's updateListener (set up in
// `main`) sees the doc change and debounce-fires `recompile`.
function setEditorSource(source) {
  editorView.dispatch({
    changes: { from: 0, to: editorView.state.doc.length, insert: source },
    effects: [
      setHighlight.of(null),
      setCurrentFn.of(null),
      setError.of(null),
    ],
  });
}

// US1: re-run the M01→M02→M03 pipeline on the current editor content and
// render the result. Called by the debounced updateListener.
function recompile(source) {
  stopPlay();
  const result = JSON.parse(player.set_source(source));
  if (result.ok) {
    render(result.state);
  } else {
    renderError(result.error);
  }
}

// US1: load a sample's source from /samples/<id>.rs into the editor. The
// updateListener picks up the doc change and triggers recompile.
async function loadSample(id) {
  stopPlay();
  const res = await fetch(`/samples/${id}.rs`);
  if (!res.ok) throw new Error(`fetch /samples/${id}.rs → ${res.status}`);
  const source = await res.text();
  setEditorSource(source);
}

// ─── Controls ─────────────────────────────────────────────────────────────

function stopPlay() {
  if (playInterval !== null) {
    clearInterval(playInterval);
    playInterval = null;
    setPlayButton("paused");
  }
}

function setPlayButton(state) {
  const btn = document.getElementById("btn-play-pause");
  btn.dataset.state = state;
  btn.textContent = state === "playing" ? "⏸ Pause" : "▶ Play";
  btn.setAttribute("aria-label", state === "playing" ? "pause" : "play");
}

function togglePlay() {
  if (playInterval !== null) {
    stopPlay();
    return;
  }
  setPlayButton("playing");
  playInterval = setInterval(() => {
    const newState = JSON.parse(player.step_forward());
    render(newState);
    if (newState.position >= newState.total) {
      stopPlay();
    }
  }, PLAY_RATE_MS);
}

// US2: Step Forward / Step Back / Rewind controls.
function wireControls() {
  document.getElementById("btn-rewind").addEventListener("click", () => {
    stopPlay();
    render(JSON.parse(player.rewind()));
  });
  document.getElementById("btn-step-back").addEventListener("click", () => {
    stopPlay();
    render(JSON.parse(player.step_back()));
  });
  document.getElementById("btn-step-forward").addEventListener("click", () => {
    stopPlay();
    render(JSON.parse(player.step_forward()));
  });
  document.getElementById("btn-play-pause").addEventListener("click", togglePlay);

  // US3: sample selector.
  document.getElementById("sample-selector").addEventListener("change", (event) => {
    loadSample(event.target.value).catch((err) => {
      console.error("loadSample failed:", err);
    });
  });
}

// ─── Main ─────────────────────────────────────────────────────────────────

async function main() {
  Player = window.wasmBindings.Player;

  // M05 / US1: debounce editor edits → recompile. The updateListener fires
  // on every doc change; we coalesce keystrokes via setTimeout/clearTimeout.
  const updateListener = EditorView.updateListener.of((update) => {
    if (!update.docChanged) return;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      recompile(update.state.doc.toString());
    }, DEBOUNCE_MS);
  });

  editorView = new EditorView({
    parent: document.getElementById("editor"),
    state: EditorState.create({
      doc: "",
      extensions: [
        rust(),
        lineNumbers(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        // M05: editor is editable. Tab inserts indentation instead of
        // navigating to the next focusable element.
        keymap.of([indentWithTab]),
        updateListener,
        highlightField,
        currentFnField,
        errorField,
      ],
    }),
  });

  // M05 / US1: Player created with empty source first; loadSample writes the
  // initial sample into the editor, the updateListener picks up the change,
  // and the debounce-fired recompile() runs the pipeline.
  player = new Player("");

  wireControls();

  // Load the first sample by default.
  await loadSample(SAMPLES[0].id);
}

// Trunk dispatches this once the WASM bindings are initialized and attached
// to `window.wasmBindings`. If our script loaded after the event already
// fired, `window.wasmBindings` will already be set — handle both cases.
function start() {
  main().catch((err) => {
    console.error("rustviz failed to start:", err);
    document.body.textContent =
      "Failed to start rustviz: " + (err.message || String(err));
  });
}

if (window.wasmBindings) {
  start();
} else {
  window.addEventListener("TrunkApplicationStarted", start, { once: true });
}
