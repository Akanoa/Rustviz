# Quickstart — Using the Milestone Roadmap

Audience: maintainer, contributors, outside observers. This file tells you how to read, audit, and revise `MILESTONES.md`.

## Read

You have ~10 minutes. Here is the order:

1. **Open `MILESTONES.md`** at the repo root.
2. **Skim the dependency graph** at the top — that tells you which milestones depend on which.
3. **Find the first milestone with `Status: planned`** in topological order. That is the *next* milestone.
4. **Read its block top to bottom**: Goal → In scope → Out of scope → Exit criteria → Demo.
5. You now know what to build next and how you will prove it is done. Done.

If you want broader context, read the `## Deferred` section too — it tells you what is *not* in the current roadmap and why.

## Audit (verify the document still reflects reality)

Run this when you suspect `CLAUDE.md` and `MILESTONES.md` have drifted (after a CLAUDE.md edit, or quarterly). Target time: under 15 minutes (SC-007).

### Step 1 — Collect CLAUDE.md scope bullets

Open `CLAUDE.md`. Treat the following as scope-bearing:

- Every bullet under `## Supported Rust subset (by levels)`.
- Every bullet under `## The three panels` (one bullet per panel + the Pointers bullet).
- Every bullet under `## Event model` (one per event category).
- Every bullet under `## Architecture` describing the three layers.
- Every numbered item under `## Immediate roadmap`.

That is roughly 20–30 bullets total.

### Step 2 — For each scope bullet, find its owner

For each bullet, search `MILESTONES.md` for the `Authority:` citation that quotes it (or paraphrases closely enough to match). Expected outcomes:

- **Owned**: exactly one milestone's `Authority` line cites this bullet. ✓
- **Deferred**: it appears under `## Deferred` with a reason. ✓
- **Orphan**: no milestone and not deferred. ✗ → action: add ownership.
- **Multi-owned**: two or more milestones claim it. ✗ → action: pick one, remove the others.

### Step 3 — Reverse check

For each `Authority` citation in `MILESTONES.md`, verify the cited CLAUDE.md section and quoted phrase still exist. If a citation references a deleted section or a phrase that was reworded:

- If CLAUDE.md changed scope intentionally → update the citation, possibly retire the milestone.
- If CLAUDE.md changed wording only → update the citation to the new wording.

### Step 4 — Sanity check the graph

- Every `Depends on:` value points to an existing `Mxx`.
- No cycles (read the graph header at the top of `MILESTONES.md`; if it's stale, redraw).
- Every milestone with `Status: closed` has a `Demo.Command` that still runs.

### Step 5 — Update `**Last audit**:`

Bump the date at the top of `MILESTONES.md`. The audit log lives in the git history.

## Revise

### Add a milestone

Pick the next free `Mxx` id (do **not** reuse old ones, even if absorbed/deferred). Insert the block in topological order. Update `Depends on:` on any later milestone that should depend on it. Redraw the graph header.

### Split a milestone

If `M06` is too big, split into `M06a` and `M06b`. Replace `M06`'s body with the absorbed-redirect template (see schema). Insert `M06a` and `M06b` immediately after, in the right order. Update graph and downstream `Depends on:` lines.

### Merge two milestones

Pick a survivor (usually the lower id). Move the absorbed one's scope into the survivor's `In scope`. Set the absorbed milestone's `Status` to `absorbed` with a redirect line. Update graph and downstream `Depends on:` lines.

### Defer a milestone

Set `Status: deferred` and move its title + reason into the `## Deferred` bullet list. The original block stays in the document with `Status: deferred` — do not delete history. (This is the one case where `deferred` items appear both as a milestone block and in `## Deferred`; the bullet list is the readable index, the block preserves rationale.)

### Reopen a deferred milestone

Set `Status: planned`, remove from the `## Deferred` bullet list. Add fresh `Authority` citations if CLAUDE.md has changed.

## Reference a milestone elsewhere

In commits, PRs, issues, and code comments, refer to a milestone by its id only: `M03`. The two-digit format makes `git log --grep '\bM03\b'` precise (R-002). Bad: "the event-model PR". Good: "M03: event-model + L1 evaluator".

## When `MILESTONES.md` and reality conflict

Reality wins. If a closed milestone's demo no longer runs, fix the demo or downgrade the status to `active` and reopen the work. If a `planned` milestone turns out to depend on something not in its `Depends on:` list, update the document before fixing the code. The document is meant to be edited; it is not sacred.
