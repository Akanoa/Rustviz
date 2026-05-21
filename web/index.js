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
import { EditorView, Decoration, lineNumbers } from "@codemirror/view";
import { syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";
import { rust } from "@codemirror/lang-rust";

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

// ─── CodeMirror span-highlight state ──────────────────────────────────────

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

// ─── State + globals ──────────────────────────────────────────────────────

let editorView = null;
let player = null;
let playInterval = null;

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

function setEditorSource(source) {
  editorView.dispatch({
    changes: { from: 0, to: editorView.state.doc.length, insert: source },
    effects: setHighlight.of(null),
  });
}

// ─── render(state) — apply a StateSnapshot to the DOM ─────────────────────

function render(state) {
  // Stacks panel: rebuild from scratch.
  const stacksEl = document.getElementById("stacks");
  stacksEl.replaceChildren();
  for (const frame of state.frames) {
    // M03.1: `active === false` means the frame has had its `FrameLeave`
    // event but we keep it visible (grayed) so the stack-bytes-persist story
    // works visually. Renderer applies the `frame-grayed` class.
    const classes = frame.active ? "frame-card" : "frame-card frame-grayed";
    const card = el("div", { class: classes });
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

  // Editor span highlight.
  if (state.editor_highlight) {
    const { start, end } = state.editor_highlight;
    editorView.dispatch({ effects: setHighlight.of({ start, end }) });
  } else {
    editorView.dispatch({ effects: setHighlight.of(null) });
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
}

// ─── Sample loading ───────────────────────────────────────────────────────

async function loadSample(id) {
  stopPlay();
  const res = await fetch(`/traces/${id}.json`);
  if (!res.ok) throw new Error(`fetch /traces/${id}.json → ${res.status}`);
  const traceText = await res.text();
  player = new Player(traceText);
  setEditorSource(player.source());
  render(JSON.parse(player.state()));
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

  editorView = new EditorView({
    parent: document.getElementById("editor"),
    state: EditorState.create({
      doc: "",
      extensions: [
        rust(),
        lineNumbers(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        EditorState.readOnly.of(true),
        EditorView.editable.of(false),
        highlightField,
      ],
    }),
  });

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
