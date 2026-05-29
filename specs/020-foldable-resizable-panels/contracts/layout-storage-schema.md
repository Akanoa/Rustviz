# Contract — Layout-storage schema

Single-key, versioned, fallback-safe JSON schema for persisting per-user panel layout in browser `localStorage`.

## Key

```
rustviz.panel-layout.v1
```

- The trailing `.v1` is a **major-version namespace**. A future v2 would write to a different key (`.v2`) so v1 saves on disk don't pollute v2 loads.
- The schema's inner `version` field is checked too; the suffix and the field MUST agree.

## Schema (v1)

```json
{
  "version": 1,
  "panels": {
    "editor": {
      "folded": false,
      "width_pct": 25,
      "user_override": false
    },
    "stacks": {
      "folded": false,
      "width_pct": 30,
      "user_override": false
    },
    "heap": {
      "folded": false,
      "width_pct": 25,
      "user_override": false
    },
    "vtables": {
      "folded": false,
      "width_pct": 10,
      "user_override": false
    },
    "static": {
      "folded": false,
      "width_pct": 10,
      "user_override": false
    }
  }
}
```

### Field semantics

| Field | Type | Required | Meaning |
|---|---|---|---|
| `version` | `1` | Yes | Schema major version. v1. Mismatch → discard. |
| `panels.<id>.folded` | `boolean` | No (default `false`) | User has explicitly folded this panel. |
| `panels.<id>.width_pct` | `number` ∈ `[5, 95]` | No (default per panel) | Last-used or saved width as percentage of `<main>`. Clamped on load. |
| `panels.<id>.user_override` | `boolean` | No (default `false`) | User has explicitly unfolded an auto-collapse-eligible panel. Sticks until reset. |

### Panel ids

Five fixed panel ids: `"editor"`, `"stacks"`, `"heap"`, `"vtables"`, `"static"`. Missing entries fall back to defaults; unknown entries are ignored.

## Behavioral guarantees

- **B-PL-1**: Loading falls back to defaults if `localStorage` is unavailable, the value is missing, parsing fails, `version !== 1`, or the shape is unrecognizable. The page renders.
- **B-PL-2**: Saving silently no-ops if `localStorage` is unavailable. The page renders correctly with in-memory state for the current session.
- **B-PL-3**: Defaults sum to 100% across all five panels (25 + 30 + 25 + 10 + 10).
- **B-PL-4**: `width_pct` is clamped to `[5, 95]` on load. A corrupt 100 or 0 doesn't break the layout.
- **B-PL-5**: An unknown future field in a v1 blob is preserved on round-trip (read → in-memory → write) so v1-aware code doesn't lose data written by a future minor-version-compatible writer. (Implementation may choose to drop unknown fields; not load-bearing.)
- **B-PL-6**: `version` MUST match `1` exactly. `0`, `2`, missing → discard.
- **B-PL-7**: The `Reset layout` action removes the key entirely; the next load uses defaults.
- **B-PL-8**: A successful save writes the COMPLETE current state (all five panels) — no partial writes. Atomic via JSON-serialize + single `localStorage.setItem`.

## Forward compatibility

A future v2 schema would:
- Write to a new key `rustviz.panel-layout.v2`.
- Read both `.v1` and `.v2`; on first save after upgrade, migrate v1 → v2 and write to `.v2` (leaving `.v1` intact briefly for downgrade safety).
- A user downgrading would still see `.v1` data and continue working at their last v1 layout.

This contract does NOT define v2 — only states the namespace pattern.

## Out of scope

- **Sync across devices** — `localStorage` is per-browser. No cloud sync.
- **Profiles** — single layout per browser. No per-user / per-sample layouts.
- **Compression** — the blob is < 1 KB; uncompressed is fine.
- **Encryption** — non-sensitive UI state; plain JSON.
