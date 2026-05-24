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
        // M07.3: also empty for arrays — replaced by inline byte-cells below.
      } else {
        valueEl.textContent = `= ${slot.value}`;
      }
      // **M07.4**: structs render a per-field labeled-rows view
      // (research R-016 Proposal A — vertical labeled rows). Built
      // INSIDE the value cell so the row still consumes exactly 3
      // grid cells (name | type | value).
      if (slot.struct_view) {
        const sv = slot.struct_view;
        const svEl = el("div", { class: "struct-view" });
        svEl.setAttribute("data-struct-name", sv.name);
        for (const f of sv.fields) {
          const fieldEl = el("div", { class: "struct-field" });
          fieldEl.setAttribute("data-field-name", f.name);
          fieldEl.appendChild(
            el("span", { class: "struct-field-label", text: `${f.name}: ${f.ty_label}` }),
          );
          const cellsEl = el("div", { class: "struct-field-cells" });
          for (let i = 0; i < f.size; i++) {
            cellsEl.appendChild(el("span", { class: "byte-cell byte-used" }));
          }
          fieldEl.appendChild(cellsEl);
          fieldEl.appendChild(
            el("span", { class: "struct-field-value", text: `= ${f.display}` }),
          );
          svEl.appendChild(fieldEl);
        }
        valueEl.appendChild(svEl);
      }
      // **M07.7**: trait-object slots render as a fat pointer with TWO
      // labeled cells (`data: → label` and `vtable: → label`) inside the
      // slot's value column. Mutually exclusive with the regular value
      // text + struct_view + inline_cells (all suppressed at apply_event
      // time for DynRef / BoxDyn values). The `dyn-cell-vtable` cell carries
      // `data-vtable-addr` so dispatch arrows (rendered transiently at
      // method-call steps) can resolve the right vtable box.
      if (slot.dyn_view) {
        const dv = slot.dyn_view;
        const dvEl = el("div", { class: "dyn-fat-pointer" });
        dvEl.setAttribute("data-vtable-addr", String(dv.vtable_addr));
        const dataCell = el("div", { class: "dyn-cell dyn-cell-data" });
        dataCell.appendChild(el("span", { class: "dyn-cell-label", text: "data:" }));
        dataCell.appendChild(el("span", { class: "dyn-cell-target", text: `→ ${dv.data_label}` }));
        const vtableCell = el("div", { class: "dyn-cell dyn-cell-vtable" });
        vtableCell.setAttribute("data-vtable-addr", String(dv.vtable_addr));
        vtableCell.appendChild(el("span", { class: "dyn-cell-label", text: "vtable:" }));
        vtableCell.appendChild(el("span", { class: "dyn-cell-target", text: `→ ${dv.vtable_label}` }));
        dvEl.appendChild(dataCell);
        dvEl.appendChild(vtableCell);
        valueEl.appendChild(dvEl);
      }
      // **M07.3**: arrays render inline byte-cells + element labels INSIDE
      // the value cell, so the row still consumes exactly 3 grid cells
      // (name | type | value) — appending them as siblings of valueEl
      // would push them into the next grid row and overflow into the
      // following slot's columns.
      if (slot.inline_cells) {
        const ic = slot.inline_cells;
        const cellsEl = el("div", { class: "stack-inline-cells" });
        for (let i = 0; i < ic.size; i++) {
          const c = el("span", { class: i < ic.used ? "byte-cell byte-used" : "byte-cell" });
          cellsEl.appendChild(c);
        }
        valueEl.appendChild(cellsEl);
        // Element labels: when there are more than INLINE_ELEM_LIMIT, show
        // the first INLINE_ELEM_LIMIT inline and a clickable "+N more"
        // toggle. Clicking expands to a vertical stack of all elements;
        // click again collapses. Threshold is chosen so common small
        // arrays (≤ 4 elements) display inline; anything bigger elides
        // and stays clean.
        const INLINE_ELEM_LIMIT = 4;
        const overflowing = ic.elements.length > INLINE_ELEM_LIMIT;
        const labelsEl = el("div", {
          class: overflowing ? "stack-elem-labels collapsed" : "stack-elem-labels",
        });
        ic.elements.forEach((label, idx) => {
          const span = el("span", { class: "elem-cell", text: label });
          span.setAttribute("data-elem-idx", String(idx));
          labelsEl.appendChild(span);
        });
        if (overflowing) {
          const hidden = ic.elements.length - INLINE_ELEM_LIMIT;
          const toggle = el("button", {
            class: "stack-elem-toggle",
            text: `+${hidden} more`,
          });
          toggle.setAttribute("type", "button");
          toggle.addEventListener("click", (ev) => {
            ev.stopPropagation();
            const expanded = labelsEl.classList.toggle("expanded");
            labelsEl.classList.toggle("collapsed", !expanded);
            toggle.textContent = expanded ? "− collapse" : `+${hidden} more`;
          });
          labelsEl.appendChild(toggle);
        }
        valueEl.appendChild(labelsEl);
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
  // **M07.2**: render the static-memory region too, for the same reason —
  // slice arrows targeting `&'static str` literals query `data-static-addr`.
  renderStaticRegion(state.static_region || []);
  renderHeap(state.heap || []);
  // **M07.7**: render the VTABLES panel. Each VtableView is one box
  // listing the trait's methods. Persists for the trace's lifetime,
  // same as static memory (content-deduplicated by `(trait, type)`).
  renderVtables(state.vtables || []);

  // M06.1 → M07: render arrows LAST, after the status bar AND heap have
  // taken their final layout. Use requestAnimationFrame so the browser has
  // flushed all DOM mutations before getBoundingClientRect.
  // **M07.2**: transient copy arrow rendered alongside the regular arrows
  // — only present on the BytesCopy cursor step (otherwise pending_copy
  // is null and renderCopyArrow is a no-op).
  requestAnimationFrame(() => {
    renderArrows(state.arrows || []);
    renderCopyArrow(state.pending_copy || null);
    renderDispatchArrow(state.pending_dispatch || null);
  });
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
    : a.target && a.target.Static !== undefined ? `t${a.target.Static}`
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
    } else if (a.target && a.target.Static !== undefined) {
      // **M07.2**: slice arrow targeting a static-memory block. Static
      // blocks live in their own region between stacks and heap; route
      // arrows to them the same way as heap targets (enter from above).
      tgtEl = document.querySelector(`[data-static-addr="${a.target.Static}"]`);
      targetIsHeap = true; // reuse the "enter from above" routing
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
      // **M07.1**: distribute arrowhead landing X across the target block's
      // top edge so multiple arrows targeting the same heap block (e.g. an
      // owning arrow from `v` PLUS a slice arrow from `&v[..]`) land at
      // different columns instead of collapsing onto the single midpoint.
      // Uses the same per-target indexing as the slot-target routing below.
      const tList = byTarget.get(targetKey(a));
      const tIdx = tList.indexOf(a);
      const xOffsetTgt = distOffset(tgt.width, tIdx, tList.length);
      const targetX = tgt.left + tgt.width / 2 + xOffsetTgt - overlayBox.left;
      const targetTopY = tgt.top - overlayBox.top;
      const laneX = Math.min(x1, targetX) - (10 + arrIdx * 6);
      d = `M${x1},${y1} H${laneX} V${laneY} H${targetX} V${targetTopY}`;
    } else {
      // Slot target: original left-gutter routing (enter from left edge).
      const tList = byTarget.get(targetKey(a));
      const tIdx = tList.indexOf(a);
      const yOffsetTgt = distOffset(tgt.height, tIdx, tList.length);
      const globalNudge = (arrIdx - (arrows.length - 1) / 2) * 4;
      // M07.4: pull the arrowhead back from the target's left edge so the
      // tip doesn't touch the box border (was visually cramped — the
      // arrowhead's flat back sat flush against the slot row, reading as
      // "stuck to" rather than "pointing at"). 6px is enough once the
      // lane offset below adds the longer horizontal stub.
      const ARROW_TIP_GAP = 6;
      const x2 = tgt.left - overlayBox.left - ARROW_TIP_GAP;
      const y2 = tgt.top + tgt.height / 2 + yOffsetTgt + globalNudge - overlayBox.top;
      // M07.4: extended the lane (gutter) offset from 10 → 24 so the final
      // H segment connecting the vertical line to the arrowhead is wider
      // — the previous ~10px stub read as "arrow stuck against gutter"
      // rather than "arrow approaches target from the left".
      const lane = 24 + arrIdx * 6;
      const gutterX = Math.min(x1, x2) - lane;
      d = `M${x1},${y1 + globalNudge} H${gutterX} V${y2} H${x2}`;
    }

    path.setAttribute("d", d);
    const cls = a.kind === "Mut" ? "arrow-mut"
              : a.kind === "Owning" ? "arrow-owning"
              : "arrow-shared";
    path.setAttribute("class", cls);
    // **M07.2**: add an invisible wider "hit-target" path with the same
    // geometry, appended BEFORE the visible path so the visible line
    // stays on top visually. SVG `pointer-events: stroke` uses the actual
    // stroke width for hit-testing, so a wider transparent stroke is the
    // only standard way to expand the hover-detection zone without
    // thickening the visible line. CSS targets `.arrow-hit-target`
    // (stroke-width 5px: ~1.5px visible + ~3.5px detection padding).
    const hitTarget = document.createElementNS(NS, "path");
    hitTarget.setAttribute("d", d);
    hitTarget.setAttribute("class", "arrow-hit-target");
    // Use :has() in CSS to apply the visual hover style (color, drop-
    // shadow, stroke-width) to the visible sibling path when the
    // hit-target is hovered. The visible path itself has
    // pointer-events:none so all hover dispatching goes through the
    // hit-target — letting the hit-target be the source of mouseenter/
    // mouseleave events for the slice-highlight handler too.
    overlay.appendChild(hitTarget);
    overlay.appendChild(path);

    // **M07.4**: hover-only arrows (method `self` receivers) start hidden
    // and reveal when the source slot row is hovered. Calling-convention
    // borrows don't deserve permanent visual weight like an explicit
    // `let r = &p.x` does.
    if (a.hover_only) {
      path.classList.add("arrow-hover-only");
      // The hit-target stays in the DOM but with no purpose for hover-only
      // arrows — the source slot row is what receives mouseenter. Keep
      // pointer-events off so an invisible hit-target doesn't capture
      // mouse events.
      hitTarget.style.pointerEvents = "none";
      const sourceRow = srcEl.closest(".slot-row");
      if (sourceRow) {
        sourceRow.addEventListener("mouseenter", () =>
          path.classList.add("arrow-visible"),
        );
        sourceRow.addEventListener("mouseleave", () =>
          path.classList.remove("arrow-visible"),
        );
      }
    }

    // **M07.1**: slice arrows carry a length annotation. When `len` is
    // present, render a small `[len: N]` label near the arrowhead.
    // **M07.2 fix**: the label is hidden by default and only revealed on
    // hover of the corresponding arrow — keeps the visualization clean
    // when many slices coexist (multiple labels overlapping with each
    // other or with adjacent block headers was confusing). Created
    // upfront so the hover handler below can toggle its visibility.
    let lenLabel = null;
    if (a.len !== undefined && a.len !== null) {
      lenLabel = document.createElementNS(NS, "text");
      let labelX, labelY;
      if (targetIsHeap) {
        const tList = byTarget.get(targetKey(a));
        const tIdx = tList.indexOf(a);
        labelX = tgt.left + tgt.width - overlayBox.left + 6;
        labelY = tgt.top - overlayBox.top + 10 + tIdx * 14;
      } else {
        const tList = byTarget.get(targetKey(a));
        const tIdx = tList.indexOf(a);
        const yOffsetTgt = distOffset(tgt.height, tIdx, tList.length);
        const globalNudge = (arrIdx - (arrows.length - 1) / 2) * 4;
        const x2 = tgt.left - overlayBox.left;
        const y2 = tgt.top + tgt.height / 2 + yOffsetTgt + globalNudge - overlayBox.top;
        labelX = x2 - 36;
        labelY = y2 - 4;
      }
      lenLabel.setAttribute("x", String(labelX));
      lenLabel.setAttribute("y", String(labelY));
      lenLabel.setAttribute("class", "arrow-len-label");
      lenLabel.textContent = `[len: ${a.len}]`;
      overlay.appendChild(lenLabel);
    }

    // **M07.4**: field-borrow arrows carry a field name annotation
    // (`.x`). Render a small label next to the arrowhead — same
    // hover-revealed pattern as `[len: N]` slice labels. Built upfront
    // so the hover handler below can toggle its visibility.
    let fieldLabelEl = null;
    if (a.field_label) {
      fieldLabelEl = document.createElementNS(NS, "text");
      // Position the field label slightly above the arrow's target end
      // (analogous to slice [len: N] positioning for Slot targets).
      const tListF = byTarget.get(targetKey(a));
      const tIdxF = tListF.indexOf(a);
      const yOffsetTgtF = distOffset(tgt.height, tIdxF, tListF.length);
      const globalNudgeF = (arrIdx - (arrows.length - 1) / 2) * 4;
      const x2F = tgt.left - overlayBox.left;
      const y2F = tgt.top + tgt.height / 2 + yOffsetTgtF + globalNudgeF - overlayBox.top;
      fieldLabelEl.setAttribute("x", String(x2F - 24));
      fieldLabelEl.setAttribute("y", String(y2F - 4));
      fieldLabelEl.setAttribute("class", "arrow-field-label");
      fieldLabelEl.textContent = a.field_label;
      overlay.appendChild(fieldLabelEl);
    }

    // **M07.4**: field-borrow arrows highlight ONLY the borrowed field's
    // row in the target struct view on hover — pedagogically: "this
    // borrow views a sub-region of the composite value". Strip the
    // leading `.` from the field label to recover the bare field name.
    if (a.field_label && a.target && a.target.Slot !== undefined) {
      const fieldName = a.field_label.startsWith(".")
        ? a.field_label.slice(1)
        : a.field_label;
      const nameEl = document.querySelector(`[data-slot-id="${a.target.Slot}"]`);
      const slotRow = nameEl ? nameEl.closest(".slot-row") : null;
      const fieldRow = slotRow
        ? slotRow.querySelector(`.struct-field[data-field-name="${fieldName}"]`)
        : null;
      const setFieldHighlight = (on) => {
        if (fieldRow) fieldRow.classList.toggle("field-borrow-highlighted", on);
        if (fieldLabelEl) fieldLabelEl.classList.toggle("label-visible", on);
      };
      hitTarget.addEventListener("mouseenter", () => setFieldHighlight(true));
      hitTarget.addEventListener("mouseleave", () => setFieldHighlight(false));
    }

    // **M07.1**: slice arrows highlight their covered region in the target
    // heap block on hover — both the byte-cells (byte_offset + byte_len)
    // AND the element-span labels (elem_start + len). For `&v[1..3]` of
    // Vec<i32>: bytes [4, 12) light up alongside elements `2_i32, 3_i32`
    // in the display string. **M07.2**: also toggles the `[len: N]`
    // label's visibility (hidden by default, shown on hover).
    // M07.3: also enable highlight for Slot targets (arrays in stack slots).
    const isSlotTarget = a.target && a.target.Slot !== undefined;
    if (
      (targetIsHeap || isSlotTarget)
      && a.byte_offset !== undefined && a.byte_offset !== null
      && a.byte_len !== undefined && a.byte_len !== null
    ) {
      // **M07.2 / M07.3**: target may be a heap block, static block, OR
      // a stack slot holding an array.
      // - Heap blocks: byte-cells in `.heap-cells`; element labels in
      //   `.heap-display` (Vec elements).
      // - Static blocks: byte-cells in `.static-cells`; byte spans in
      //   `.static-display` (1 byte = 1 char for ASCII).
      // - Slot (M07.3, array): byte-cells in `.stack-inline-cells`;
      //   element labels in `.stack-elem-labels` (per-element strings).
      const isStatic = a.target.Static !== undefined;
      let targetBox = null;
      if (isStatic) {
        targetBox = document.querySelector(`[data-static-addr="${a.target.Static}"]`);
      } else if (isSlotTarget) {
        // For slot targets, `data-slot-id` is on the .slot-name span;
        // its enclosing .slot-row holds the .stack-inline-cells +
        // .stack-elem-labels children.
        const nameEl = document.querySelector(`[data-slot-id="${a.target.Slot}"]`);
        targetBox = nameEl ? nameEl.closest(".slot-row") : null;
      } else {
        targetBox = document.querySelector(`[data-heap-addr="${a.target.Heap}"]`);
      }
      const cellsEl = targetBox
        ? targetBox.querySelector(
            isStatic ? ".static-cells"
            : isSlotTarget ? ".stack-inline-cells"
            : ".heap-cells",
          )
        : null;
      const dispEl = targetBox
        ? targetBox.querySelector(
            isStatic ? ".static-display"
            : isSlotTarget ? ".stack-elem-labels"
            : ".heap-display",
          )
        : null;
      const byteStart = Number(a.byte_offset);
      const byteEnd = byteStart + Number(a.byte_len);
      // For Vec/Array slices, element-span highlight uses elem_start + len.
      // For static slices, it uses byte_offset + byte_len (1:1 byte/char).
      const [elemStart, elemEnd] = isStatic
        ? [byteStart, byteEnd]
        : (a.elem_start !== undefined && a.elem_start !== null
            && a.len !== undefined && a.len !== null
            ? [Number(a.elem_start), Number(a.elem_start) + Number(a.len)]
            : [null, null]);
      const setHighlight = (on) => {
        if (cellsEl) {
          for (let i = byteStart; i < byteEnd && i < cellsEl.children.length; i++) {
            cellsEl.children[i].classList.toggle("byte-slice-highlighted", on);
          }
        }
        if (dispEl && elemStart !== null && elemEnd !== null) {
          for (let i = elemStart; i < elemEnd; i++) {
            const span = dispEl.querySelector(`[data-elem-idx="${i}"]`);
            if (span) span.classList.toggle("elem-slice-highlighted", on);
          }
        }
        // M07.2: reveal the [len: N] label on hover; hide on leave.
        if (lenLabel) lenLabel.classList.toggle("label-visible", on);
      };
      // M07.2: events fire on the wider hit-target, not the visible path.
      hitTarget.addEventListener("mouseenter", () => setHighlight(true));
      hitTarget.addEventListener("mouseleave", () => setHighlight(false));
    } else if (lenLabel) {
      // Slice arrow without byte-range (shouldn't happen post-M07.1, but
      // be defensive): still wire label visibility to hover.
      hitTarget.addEventListener("mouseenter", () => lenLabel.classList.add("label-visible"));
      hitTarget.addEventListener("mouseleave", () => lenLabel.classList.remove("label-visible"));
    }
  }
}

// **M07.2**: render the transient "bytes copied" arrow. Fires only on the
// cursor step where a `BytesCopy` event is current — `pending_copy` is
// null at all other steps. The arrow is orange + dashed + auto-fades in,
// visually distinct from blue/red/black ownership/borrow arrows so the
// learner reads it as "data flow", not "permanent pointer". A small
// "copy N bytes" label sits alongside. Also highlights the source
// byte-cells AND char spans covered by the copy so the learner sees
// exactly which bytes flowed.
function renderCopyArrow(pendingCopy) {
  // Always clear any stale copy-source highlights from the previous step.
  for (const el of document.querySelectorAll(".byte-copy-source-highlighted")) {
    el.classList.remove("byte-copy-source-highlighted");
  }
  for (const el of document.querySelectorAll(".elem-copy-source-highlighted")) {
    el.classList.remove("elem-copy-source-highlighted");
  }
  if (!pendingCopy) return;
  const overlay = document.getElementById("arrow-overlay");
  if (!overlay) return;
  const NS = "http://www.w3.org/2000/svg";

  // Resolve source DOM element.
  let srcEl = null;
  let srcIsStatic = false;
  if (pendingCopy.from.Slot !== undefined) {
    srcEl = document.querySelector(`[data-slot-id="${pendingCopy.from.Slot}"]`);
  } else if (pendingCopy.from.Heap !== undefined) {
    srcEl = document.querySelector(`[data-heap-addr="${pendingCopy.from.Heap}"]`);
  } else if (pendingCopy.from.Static !== undefined) {
    srcEl = document.querySelector(`[data-static-addr="${pendingCopy.from.Static}"]`);
    srcIsStatic = true;
  }
  const tgtEl = document.querySelector(`[data-heap-addr="${pendingCopy.to}"]`);
  if (!srcEl || !tgtEl) return;

  const overlayBox = overlay.getBoundingClientRect();
  const src = srcEl.getBoundingClientRect();
  const tgt = tgtEl.getBoundingClientRect();

  // Route: a direct angled line from the source's right edge to the
  // target's left edge. Curved (quadratic bezier) for a "flowing" feel.
  const x1 = src.right - overlayBox.left;
  const y1 = src.top + src.height / 2 - overlayBox.top;
  const x2 = tgt.left - overlayBox.left;
  const y2 = tgt.top + tgt.height / 2 - overlayBox.top;
  // Control point: midpoint, bowed downward slightly so the curve doesn't
  // overlap straight horizontal arrows above.
  const mx = (x1 + x2) / 2;
  const my = (y1 + y2) / 2 + 20;
  const d = `M${x1},${y1} Q${mx},${my} ${x2},${y2}`;

  const path = document.createElementNS(NS, "path");
  path.setAttribute("d", d);
  path.setAttribute("class", "arrow-copy");
  overlay.appendChild(path);

  // "copy N bytes" label near the curve's apex.
  const label = document.createElementNS(NS, "text");
  label.setAttribute("x", String(mx));
  label.setAttribute("y", String(my + 14));
  label.setAttribute("class", "arrow-copy-label");
  label.setAttribute("text-anchor", "middle");
  label.textContent = `copy ${pendingCopy.n_bytes} byte${pendingCopy.n_bytes === 1 ? "" : "s"}`;
  overlay.appendChild(label);

  // Highlight the source bytes/chars covered by this copy. The byte-cells
  // are inside `.heap-cells` (heap source) or `.static-cells` (static
  // source). For static sources, also highlight the char spans inside
  // `.static-display` since static blocks segment their display per byte.
  const cellsSelector = srcIsStatic ? ".static-cells" : ".heap-cells";
  const cellsEl = srcEl.querySelector(cellsSelector);
  const byteStart = Number(pendingCopy.from_byte_offset);
  const byteEnd = byteStart + Number(pendingCopy.n_bytes);
  if (cellsEl) {
    for (let i = byteStart; i < byteEnd && i < cellsEl.children.length; i++) {
      cellsEl.children[i].classList.add("byte-copy-source-highlighted");
    }
  }
  if (srcIsStatic) {
    const dispEl = srcEl.querySelector(".static-display");
    if (dispEl) {
      for (let i = byteStart; i < byteEnd; i++) {
        const span = dispEl.querySelector(`[data-elem-idx="${i}"]`);
        if (span) span.classList.add("elem-copy-source-highlighted");
      }
    }
  }
}

// **M07.7**: render the transient trait-object dispatch indicator. Fires
// only at the FrameEnter cursor step for a `<Type as Trait>::method`
// dispatch where a caller slot holds the matching DynRef/BoxDyn. Draws:
//   1. ONE dashed-orange arrow from the source slot's `dyn-cell-vtable`
//      directly to the new method frame card.
//   2. A highlight on the matching method row inside the vtable box —
//      conveys "the vtable resolved THIS method" without a second arrow.
// Cleared on the next cursor step (transient, same lifecycle as the
// BytesCopy arrow).
function renderDispatchArrow(pendingDispatch) {
  // Always clear stale method-row highlights from previous step.
  for (const el of document.querySelectorAll(".vtable-method.vtable-method-highlighted")) {
    el.classList.remove("vtable-method-highlighted");
  }
  if (!pendingDispatch) return;
  const overlay = document.getElementById("arrow-overlay");
  if (!overlay) return;
  const NS = "http://www.w3.org/2000/svg";

  const sourceSlot = pendingDispatch.source_slot;
  const vtableAddr = pendingDispatch.vtable_addr;
  const method = pendingDispatch.method;

  const slotEl = document.querySelector(`[data-slot-id="${sourceSlot}"]`);
  if (!slotEl) return;
  const slotRow = slotEl.closest(".slot-row");
  if (!slotRow) return;
  const vtableCell = slotRow.querySelector(".dyn-cell-vtable");
  const vtableBox = document.querySelector(`.vtable-box[data-vtable-addr="${vtableAddr}"]`);
  const frameCard = document.querySelector(".frame-card.frame-current");

  // Highlight the matching method row inside the vtable box (always, even
  // when the arrow paths can't render). This is the "indirection step"
  // visual cue — the vtable resolved this specific method.
  if (vtableBox) {
    const methodRow = vtableBox.querySelector(`.vtable-method[data-method="${method}"]`);
    if (methodRow) {
      methodRow.classList.add("vtable-method-highlighted");
    }
  }

  if (!vtableCell || !frameCard) return;

  const overlayBox = overlay.getBoundingClientRect();
  const dst = frameCard.getBoundingClientRect();

  // Anchor the arrow source at the SLOT NAME element (same anchor as
  // borrow arrows) so dispatch arrows share the visual vocabulary of
  // other arrows in the panel — they all "leave the slot from the left".
  const slotNameSrc = slotEl.getBoundingClientRect();

  // Right-angle path through a dedicated left gutter wider than the
  // borrow gutter (24px) so dispatch arrows don't overlap any borrow
  // arrows that share the source slot. Distinctive dashed-orange style
  // disambiguates anyway, but the wider lane keeps the geometry clean.
  const ARROW_TIP_GAP = 6;
  const x1 = slotNameSrc.left - overlayBox.left;
  const y1 = slotNameSrc.top + slotNameSrc.height / 2 - overlayBox.top;
  const x2 = dst.left - overlayBox.left - ARROW_TIP_GAP;
  const y2 = dst.top + dst.height / 2 - overlayBox.top;
  const lane = 36;
  const gutterX = Math.min(x1, x2) - lane;
  const d = `M${x1},${y1} H${gutterX} V${y2} H${x2}`;
  const path = document.createElementNS(NS, "path");
  path.setAttribute("d", d);
  path.setAttribute("class", "arrow-vtable-dispatch");
  path.setAttribute("marker-end", "url(#arrow-head-vtable)");
  overlay.appendChild(path);

  // Method label near the arrowhead, just left of the dispatch frame card.
  const label = document.createElementNS(NS, "text");
  label.setAttribute("x", String(x2 - 4));
  label.setAttribute("y", String(y2 - 6));
  label.setAttribute("class", "arrow-vtable-dispatch-label");
  label.setAttribute("text-anchor", "end");
  label.textContent = `dispatch: ${method}`;
  overlay.appendChild(label);
}

// **M07.1**: render a heap-box's display, segmenting Vec elements into
// `<span data-elem-idx="i">` so the slice hover handler can highlight
// individual elements. Match `Vec [e0, e1, ...] (cap=N, len=N)`. For
// other shapes (Box, String) fall back to plain text.
//
// The element splitting uses a balanced-bracket walker rather than a naive
// split on `,` so element renderings that themselves contain commas (none
// today, but a defensive choice) wouldn't break the segmentation. Elements
// are trimmed individually.
function renderHeapDisplay(dispEl, display) {
  dispEl.textContent = ""; // clear any prior contents (textNodes + spans)
  const vecMatch = display.match(/^(Vec )\[(.*)\]( \(.*\))?$/);
  if (!vecMatch) {
    dispEl.textContent = display;
    return;
  }
  const [, prefix, inner, suffix] = vecMatch;
  dispEl.appendChild(document.createTextNode(prefix + "["));
  // Split inner on top-level commas (defensive: handles nested brackets).
  const parts = [];
  let depth = 0, start = 0;
  for (let i = 0; i < inner.length; i++) {
    const c = inner[i];
    if (c === "[" || c === "(" || c === "{") depth++;
    else if (c === "]" || c === ")" || c === "}") depth--;
    else if (c === "," && depth === 0) {
      parts.push(inner.slice(start, i));
      start = i + 1;
    }
  }
  if (start < inner.length) parts.push(inner.slice(start));
  parts.forEach((part, idx) => {
    if (idx > 0) dispEl.appendChild(document.createTextNode(", "));
    const span = document.createElement("span");
    span.className = "elem-cell";
    span.setAttribute("data-elem-idx", String(idx));
    span.textContent = part.trim();
    dispEl.appendChild(span);
  });
  dispEl.appendChild(document.createTextNode("]" + (suffix || "")));
}

// **M07.2**: render a static block's display, segmenting each byte into
// `<span class="elem-cell" data-elem-idx="i">` so the slice-hover handler
// can highlight individual bytes. Surrounding quotes are plain text so
// they don't get highlighted (only the inner bytes do).
function renderStaticDisplay(dispEl, bytes) {
  dispEl.textContent = ""; // clear prior contents
  dispEl.appendChild(document.createTextNode("\""));
  for (let i = 0; i < bytes.length; i++) {
    const span = document.createElement("span");
    span.className = "elem-cell";
    span.setAttribute("data-elem-idx", String(i));
    // Render visible escape sequences for the common pedagogically-relevant
    // bytes (newline, tab, backslash, quote) so they show as `\n` rather
    // than collapsing into whitespace in the UI.
    const c = bytes[i];
    if (c === "\n") span.textContent = "\\n";
    else if (c === "\t") span.textContent = "\\t";
    else if (c === "\\") span.textContent = "\\\\";
    else if (c === "\"") span.textContent = "\\\"";
    else span.textContent = c;
    dispEl.appendChild(span);
  }
  dispEl.appendChild(document.createTextNode("\""));
}

// **M07.2**: render the static-memory region. Each StaticView is one
// read-only block holding a string literal's bytes. Persists for the
// trace's lifetime — once a block is rendered, it stays. Re-used across
// renders via `staticElements: Map<addr, HTMLElement>` so slice arrows
// targeting `data-static-addr` can resolve consistently.
const staticElements = new Map();
function renderStaticRegion(staticRegion) {
  const region = document.getElementById("static");
  if (!region) return;
  // Find or create the inner container (header sibling).
  let container = region.querySelector(".static-blocks");
  if (!container) {
    container = document.createElement("div");
    container.className = "static-blocks";
    region.appendChild(container);
  }
  const seenAddrs = new Set();
  for (const s of staticRegion) {
    seenAddrs.add(s.addr);
    let block = staticElements.get(s.addr);
    if (!block) {
      block = document.createElement("div");
      block.className = "static-block";
      block.setAttribute("data-static-addr", String(s.addr));
      const addr = document.createElement("div");
      addr.className = "static-addr";
      addr.textContent = `static #${s.addr} (${s.size}B)`;
      const disp = document.createElement("div");
      disp.className = "static-display";
      const cells = document.createElement("div");
      cells.className = "static-cells";
      block.appendChild(addr);
      block.appendChild(disp);
      block.appendChild(cells);
      container.appendChild(block);
      staticElements.set(s.addr, block);
    }
    // **M07.2**: segment each byte of the string into its own
    // `<span data-elem-idx="i">` so the slice-hover handler can light up
    // the bytes covered by the slice (e.g. hovering `&s[..2]` highlights
    // `he` in `"hello"`). The surrounding quotes are rendered as plain
    // text. For ASCII (the only case M07.2 handles), 1 byte = 1 displayed
    // char, so byte index = element index directly.
    renderStaticDisplay(block.querySelector(".static-display"), s.bytes);
    // Byte-cells: one per byte, all filled (static blocks have no
    // "capacity vs used" distinction — every byte is real content).
    const cellsEl = block.querySelector(".static-cells");
    while (cellsEl.children.length < s.size) {
      const c = document.createElement("span");
      c.className = "byte-cell";
      cellsEl.appendChild(c);
    }
    while (cellsEl.children.length > s.size) {
      cellsEl.removeChild(cellsEl.lastChild);
    }
  }
  // Static blocks NEVER disappear — once interned, they persist.
  // No cleanup phase needed (in contrast to renderHeap which removes
  // freed blocks). If `staticRegion` shrinks across re-renders (e.g.
  // due to cursor rewind), elements stay in DOM but won't be in the
  // current snapshot. This matches the "static memory is forever"
  // pedagogy AND avoids fighting the player cursor on rewind — when
  // the cursor moves backward past a StaticAlloc event, the static
  // block stays visible because it'll appear again on forward step.
  //
  // Actually we DO need rewind support: on rewind, the snapshot won't
  // include that block; remove stale DOM elements so the visualization
  // matches the trace state.
  for (const [addr, el] of [...staticElements.entries()]) {
    if (!seenAddrs.has(addr)) {
      el.remove();
      staticElements.delete(addr);
    }
  }
}

// **M07.7**: render the VTABLES panel. Each VtableView is one box
// listing the trait's methods. Re-used across renders via a per-addr
// DOM-element map so the same vtable box stays in place (vtables never
// move once allocated — content-deduplicated like static memory).
const vtableElements = new Map();
function renderVtables(vtables) {
  const panel = document.getElementById("vtables");
  if (!panel) return;
  let container = panel.querySelector(".vtable-blocks");
  if (!container) {
    container = document.createElement("div");
    container.className = "vtable-blocks";
    panel.appendChild(container);
  }
  const seenAddrs = new Set();
  for (const v of vtables) {
    seenAddrs.add(v.addr);
    let box = vtableElements.get(v.addr);
    if (!box) {
      box = document.createElement("div");
      box.className = "vtable-box";
      box.setAttribute("data-vtable-addr", String(v.addr));
      const headerEl = document.createElement("div");
      headerEl.className = "vtable-header";
      headerEl.textContent = `<${v.type_name} as ${v.trait_name}>`;
      const methods = document.createElement("div");
      methods.className = "vtable-methods";
      for (const [name, target] of v.methods) {
        const row = document.createElement("div");
        row.className = "vtable-method";
        row.setAttribute("data-method", name);
        row.textContent = `${name} → ${target}`;
        methods.appendChild(row);
      }
      box.appendChild(headerEl);
      box.appendChild(methods);
      container.appendChild(box);
      vtableElements.set(v.addr, box);
    }
  }
  // Cleanup stale entries on cursor rewind (vtables never disappear in
  // forward execution, but rewinding can rewind past their VtableAlloc).
  for (const [addr, el] of [...vtableElements.entries()]) {
    if (!seenAddrs.has(addr)) {
      el.remove();
      vtableElements.delete(addr);
    }
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
      const cells = document.createElement("div");
      cells.className = "heap-cells";
      box.appendChild(addr);
      box.appendChild(disp);
      box.appendChild(cells);
      heapEl.appendChild(box);
      heapElements.set(h.addr, box);
    }
    box.setAttribute("data-heap-addr", String(h.addr));
    box.querySelector(".heap-addr").textContent =
      h.freed ? `heap #${h.addr} (freed, ${h.size}B)` : `heap #${h.addr} (${h.size}B)`;
    // **M07.1**: for Vec displays (format: `Vec [e0, e1, ...] (cap=N, len=N)`),
    // segment each element into a `<span data-elem-idx="i">` so the slice
    // hover handler can light up the elements covered by `[elem_start,
    // elem_start + len)`. For Box / String / other shapes the display is
    // rendered as plain text.
    const dispEl = box.querySelector(".heap-display");
    renderHeapDisplay(dispEl, h.display);
    box.classList.toggle("heap-freed", !!h.freed);
    // **M07**: byte-level cells. One cell per byte of total capacity.
    // First `used` cells filled (current value); rest empty (allocated
    // but unused). Makes per-type physical size differences obvious:
    // Box<f32> = 4 cells, Box<f64> = 8 cells, Vec<i32> cap=4 = 16 cells,
    // Vec<u8> cap=4 = 4 cells.
    const cellsEl = box.querySelector(".heap-cells");
    const wantCells = h.size;
    while (cellsEl.children.length < wantCells) {
      const c = document.createElement("span");
      c.className = "byte-cell";
      cellsEl.appendChild(c);
    }
    while (cellsEl.children.length > wantCells) {
      cellsEl.removeChild(cellsEl.lastChild);
    }
    for (let i = 0; i < cellsEl.children.length; i++) {
      cellsEl.children[i].className = i < h.used ? "byte-cell byte-used" : "byte-cell";
    }
  }
  // Remove DOM elements for addrs that no longer exist (HeapFree).
  for (const [addr, el] of [...heapElements.entries()]) {
    if (!seenAddrs.has(addr)) {
      el.remove();
      heapElements.delete(addr);
    }
  }
  // **M07**: reorder heap-box DOM children to match state.heap's order.
  // appendChild on an EXISTING child moves it to the end — iterating
  // state.heap and re-appending each box in order rebuilds the panel's
  // child sequence (so a split fragment inserted in state.heap right
  // after its parent ends up adjacent in the DOM too).
  for (const h of heap) {
    const box = heapElements.get(h.addr);
    if (box) heapEl.appendChild(box);
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
  const el = document.createElement("span");
  el.id = "build-id";
  el.textContent = `build: css ${cssHash} / wasm ${wasmHash}`;
  el.title = "Click to copy";
  el.style.cssText =
    "margin-left:0.75rem; font-size:10px; color:#777; " +
    "font-family:ui-monospace,monospace; user-select:text; cursor:copy; " +
    "background:rgba(0,0,0,0.04); border:1px solid #ccc; " +
    "padding:1px 6px; border-radius:3px; vertical-align:middle;";
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
  // Append next to the <h1>rustviz</h1> title in the header.
  const title = document.querySelector("header h1");
  if (title) {
    title.appendChild(el);
  } else {
    document.body.appendChild(el);
  }
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
