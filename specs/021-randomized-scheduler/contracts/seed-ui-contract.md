# Contract — Seed UI surface

## DOM additions to toolbar

```html
<!-- In the existing #toolbar footer, BEFORE the step indicator -->
<label for="seed-input" class="seed-label">seed</label>
<input id="seed-input" type="number" min="0" max="4294967295" value="0" step="1" />
<button id="btn-reroll-seed" type="button" aria-label="Generate new random seed" title="New random seed">🎲</button>
```

## JS contract

```js
const SEED_DEBOUNCE_MS = 300;
let seedDebounceTimer = null;
let currentSeed = 0;

function wireSeedControls() {
  const input = document.getElementById("seed-input");
  const btn = document.getElementById("btn-reroll-seed");
  input.addEventListener("input", onSeedInput);
  btn.addEventListener("click", onRerollClick);
}

function onSeedInput(ev) {
  clearTimeout(seedDebounceTimer);
  seedDebounceTimer = setTimeout(() => {
    const raw = parseInt(ev.target.value, 10);
    if (!Number.isFinite(raw) || raw < 0 || raw > 0xFFFFFFFF) {
      // Revert to last valid; don't re-run
      ev.target.value = String(currentSeed);
      return;
    }
    currentSeed = raw;
    rerunWithSeed(raw);
  }, SEED_DEBOUNCE_MS);
}

function onRerollClick() {
  clearTimeout(seedDebounceTimer);
  const fresh = Math.floor(Math.random() * 0x1_0000_0000);
  document.getElementById("seed-input").value = String(fresh);
  currentSeed = fresh;
  rerunWithSeed(fresh);
}

function rerunWithSeed(seed) {
  const source = editorView.state.doc.toString();
  const result = JSON.parse(player.set_source(source, seed));
  if (result.ok) {
    render(result.state);
  } else {
    renderError(result.error);
  }
}
```

## Behavioral contract

- **B-UI-1**: On page load, seed input value is `0`. The first trace renders with `seed=0`.
- **B-UI-2**: Typing into the seed input triggers a re-run after 300ms (debounced). Backspace + retype within 300ms doesn't trigger an extra render.
- **B-UI-3**: Clicking the re-roll button generates a fresh seed via `Math.random() * 2^32 | 0`, updates the input value, AND triggers a re-run IMMEDIATELY (no debounce).
- **B-UI-4**: Non-numeric input (e.g., letters, negative, out-of-range) reverts the input to the last valid seed without triggering a re-run.
- **B-UI-5**: After a successful render, the seed input value MUST equal `state.seed` (the seed that produced the rendered trace). The render code re-syncs the input on every state push.
- **B-UI-6**: Editing the source code while a seed is set uses the CURRENT seed for the new trace. The seed is "sticky" — changing source doesn't reset it.
- **B-UI-7**: Loading a different sample from the dropdown uses the CURRENT seed for the new sample. (Open question: should sample-switch reset seed to 0? Recommendation: NO — let the learner explore the same seed across samples to compare scheduler behavior.)
- **B-UI-8**: The seed input is reachable via tab navigation (between the play controls and the step indicator). The re-roll button is the next tab stop.
- **B-UI-9**: Both controls are disabled while a parse / typeck error is being displayed (consistent with the existing playback controls' disabled state). Re-enabled on next successful render.

## Styling contract (CSS)

```css
.seed-label {
  font-family: ui-monospace, SFMono-Regular, monospace;
  font-size: 11px;
  color: var(--muted);
  margin-left: 1rem;
}

#seed-input {
  width: 6em;                /* fits up to 10 digits (2^32 = 4294967296) */
  padding: 2px 6px;
  font-family: ui-monospace, SFMono-Regular, monospace;
  font-size: 12px;
  border: 1px solid var(--border);
  border-radius: 3px;
  background: white;
}

#btn-reroll-seed {
  background: transparent;
  border: 1px solid var(--border);
  border-radius: 3px;
  padding: 2px 6px;
  font-size: 14px;
  cursor: pointer;
  line-height: 1;
}

#btn-reroll-seed:hover {
  background: var(--frame-bg);
}
```

## Out of scope

- **URL persistence**: seed not encoded in `window.location.search`. Future enhancement.
- **Seed history / undo**: no list of previously-used seeds. Learner copies/notes them manually if they want to return.
- **Per-sample default seeds**: all samples start at seed=0 on first load. No per-sample memory.
