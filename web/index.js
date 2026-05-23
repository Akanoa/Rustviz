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
      // M06: data-slot-id lives on the .slot-name child (the row itself uses
      // `display: contents` so it has no own bounding box — getBoundingClientRect
      // on the row returns zero coords. Anchor on a child that has real layout).
      const nameEl = el("span", { class: "slot-name", text: slot.name });
      nameEl.setAttribute("data-slot-id", String(slot.slot_id));
      row.appendChild(nameEl);
      row.appendChild(el("span", { class: "slot-ty", text: `: ${slot.ty}` }));
      const valueEl = el("span", { class: "slot-value" });
      if (slot.value === null || slot.value === undefined) {
        valueEl.classList.add("slot-pending");
      } else if (slot.value === "") {
        // M07: empty value cell for heap-owning slots. The owning arrow +
        // the type column carry the pointer info; no `= ...` text needed.
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

  // **M07**: render the heap panel BEFORE arrows so the heap-box DOM
  // elements exist when renderArrows queries `data-heap-addr` positions.
  renderHeap(state.heap || []);

  // M06.1 → M07: render arrows LAST, after the status bar AND heap have
  // taken their final layout. Use requestAnimationFrame so the browser has
  // flushed all DOM mutations before getBoundingClientRect.
  requestAnimationFrame(() => renderArrows(state.arrows || []));
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

// **M06 → M07**: render arrows for both borrows and ownership. Each ArrowView
// has source_slot (always a slot id), target (Slot(id) | Heap(addr)), and
// kind (Shared | Mut | Owning). Targets are queried via data-slot-id or
// data-heap-addr respectively. Path is rectilinear via a left gutter.
function renderArrows(arrows) {
  const overlay = document.getElementById("arrow-overlay");
  if (!overlay) return;
  // Clear previous arrows (everything except the <defs>).
  for (const child of [...overlay.children]) {
    if (child.tagName.toLowerCase() !== "defs") overlay.removeChild(child);
  }
  if (!arrows || arrows.length === 0) return;

  const overlayBox = overlay.getBoundingClientRect();
  const NS = "http://www.w3.org/2000/svg";

  // M07 cosmetic: group arrows by target so we can vertically distribute
  // multiple arrows ending at the same DOM element (otherwise their final
  // H segments overlap into a single line). Same idea for source.
  const targetKey = (a) =>
    a.target && a.target.Slot !== undefined ? `s${a.target.Slot}`
    : a.target && a.target.Heap !== undefined ? `h${a.target.Heap}`
    : "?";
  const sourceKey = (a) => `s${a.source_slot}`;
  const byTarget = new Map();
  const bySource = new Map();
  for (const a of arrows) {
    const tk = targetKey(a);
    if (!byTarget.has(tk)) byTarget.set(tk, []);
    byTarget.get(tk).push(a);
    const sk = sourceKey(a);
    if (!bySource.has(sk)) bySource.set(sk, []);
    bySource.get(sk).push(a);
  }
  // Distribute n arrows vertically across a box of height h: with 1 arrow
  // anchor at center; with 2+ space them evenly avoiding edges.
  const distOffset = (h, i, n) => {
    if (n <= 1) return 0;
    const slot = h / (n + 1);
    return slot * (i + 1) - h / 2;
  };

  for (const a of arrows) {
    const srcEl = document.querySelector(`[data-slot-id="${a.source_slot}"]`);
    // M07: target can be Slot(id) or Heap(addr). The wire format is
    // { "Slot": <id> } or { "Heap": <addr> } (serde tag-by-key).
    let tgtEl = null;
    let targetIsHeap = false;
    if (a.target && a.target.Slot !== undefined) {
      tgtEl = document.querySelector(`[data-slot-id="${a.target.Slot}"]`);
    } else if (a.target && a.target.Heap !== undefined) {
      tgtEl = document.querySelector(`[data-heap-addr="${a.target.Heap}"]`);
      targetIsHeap = true;
    }
    if (!srcEl || !tgtEl) continue;
    const src = srcEl.getBoundingClientRect();
    const tgt = tgtEl.getBoundingClientRect();

    // Per-arrow distribution at source end (co-sourced arrows space out
    // vertically across the source slot's height).
    const sList = bySource.get(sourceKey(a));
    const sIdx = sList.indexOf(a);
    const yOffsetSrc = distOffset(src.height, sIdx, sList.length);
    const x1 = src.left - overlayBox.left;
    const y1 = src.top + src.height / 2 + yOffsetSrc - overlayBox.top;

    const arrIdx = arrows.indexOf(a);
    const path = document.createElementNS(NS, "path");
    let d;

    if (targetIsHeap) {
      // **M07**: enter heap target from ABOVE. Closest lane is heap.top − 20
      // — empirically the arrowhead marker (6×6 px with refX=9) needs at
      // least ~15px of clean V segment so its body doesn't overlap the
      // H→V bend (which made some arrows look like a T-shaped tip).
      // +6 per additional arrow keeps lanes from merging when multiple
      // arrows share the heap row.
      const laneY = tgt.top - 20 - arrIdx * 6 - overlayBox.top;
      const targetX = tgt.left + tgt.width / 2 - overlayBox.left;
      const targetTopY = tgt.top - overlayBox.top;
      const laneX = Math.min(x1, targetX) - (10 + arrIdx * 6);
      d = `M${x1},${y1} H${laneX} V${laneY} H${targetX} V${targetTopY}`;
    } else {
      // Slot target: original left-gutter routing (enter from left edge).
      const tList = byTarget.get(targetKey(a));
      const tIdx = tList.indexOf(a);
      const yOffsetTgt = distOffset(tgt.height, tIdx, tList.length);
      const globalNudge = (arrIdx - (arrows.length - 1) / 2) * 4;
      const x2 = tgt.left - overlayBox.left;
      const y2 = tgt.top + tgt.height / 2 + yOffsetTgt + globalNudge - overlayBox.top;
      const lane = 10 + arrIdx * 6;
      const gutterX = Math.min(x1, x2) - lane;
      d = `M${x1},${y1 + globalNudge} H${gutterX} V${y2} H${x2}`;
    }

    path.setAttribute("d", d);
    const cls = a.kind === "Mut" ? "arrow-mut"
              : a.kind === "Owning" ? "arrow-owning"
              : "arrow-shared";
    path.setAttribute("class", cls);
    overlay.appendChild(path);
  }
}

// **M07**: render the heap panel. Each HeapView in state.heap becomes a
// labeled box. Re-used across renders via a per-addr DOM-element map so
// CSS transitions animate movement on realloc.
const heapElements = new Map();
function renderHeap(heap) {
  const heapEl = document.getElementById("heap");
  if (!heapEl) return;
  const seenAddrs = new Set();
  for (const h of heap) {
    seenAddrs.add(h.addr);
    let box = heapElements.get(h.addr);
    if (!box) {
      box = document.createElement("div");
      box.className = "heap-box";
      box.setAttribute("data-heap-addr", String(h.addr));
      const addr = document.createElement("div");
      addr.className = "heap-addr";
      addr.textContent = `heap #${h.addr}`;
      const disp = document.createElement("div");
      disp.className = "heap-display";
      box.appendChild(addr);
      box.appendChild(disp);
      heapEl.appendChild(box);
      heapElements.set(h.addr, box);
    }
    // Update content + addr label every render (covers in-place updates).
    box.setAttribute("data-heap-addr", String(h.addr));
    box.querySelector(".heap-addr").textContent =
      h.freed ? `heap #${h.addr} (freed)` : `heap #${h.addr}`;
    box.querySelector(".heap-display").textContent = h.display;
    box.classList.toggle("heap-freed", !!h.freed);
  }
  // Remove DOM elements for addrs that no longer exist (HeapFree).
  for (const [addr, el] of [...heapElements.entries()]) {
    if (!seenAddrs.has(addr)) {
      el.remove();
      heapElements.delete(addr);
    }
  }
}

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

// **Cache-debug aid**: display the hashes of the currently-loaded CSS and
// WASM bootstrap. Compare against `web/dist/style-*.css` / `web/dist/*.js`
// filenames on disk to confirm the browser has the latest build.
function showBuildId() {
  const cssLink = document.querySelector('link[rel="stylesheet"]');
  const cssMatch = cssLink?.href?.match(/style-([a-f0-9]+)\.css/);
  // Trunk's WASM bootstrap is loaded as an inline module + modulepreload;
  // the modulepreload link carries the hashed filename.
  const preload = document.querySelector('link[rel="modulepreload"][href*="rustviz-"]');
  const wasmMatch = preload?.href?.match(/rustviz-([a-f0-9]+)\.js/);
  const cssHash = cssMatch ? cssMatch[1].slice(0, 8) : "?";
  const wasmHash = wasmMatch ? wasmMatch[1].slice(0, 8) : "?";
  const el = document.createElement("div");
  el.id = "build-id";
  el.textContent = `build: css ${cssHash} / wasm ${wasmHash}`;
  el.title = "Click to copy";
  el.style.cssText =
    "position:fixed; bottom:4px; right:4px; font-size:10px; color:#555; " +
    "font-family:ui-monospace,monospace; user-select:text; cursor:copy; " +
    "z-index:9999; background:rgba(255,255,255,0.85); " +
    "border:1px solid #ccc; padding:2px 6px; border-radius:3px;";
  el.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(el.textContent);
      const old = el.textContent;
      el.textContent = "✓ copied";
      setTimeout(() => { el.textContent = old; }, 800);
    } catch (e) {
      // Fallback: select the text so the user can Ctrl+C.
      const range = document.createRange();
      range.selectNode(el);
      window.getSelection().removeAllRanges();
      window.getSelection().addRange(range);
    }
  });
  document.body.appendChild(el);
}

if (window.wasmBindings) {
  start();
  showBuildId();
} else {
  window.addEventListener("TrunkApplicationStarted", () => {
    start();
    showBuildId();
  }, { once: true });
}
