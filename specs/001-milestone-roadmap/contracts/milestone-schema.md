# Contract — `MILESTONES.md` document schema

This contract specifies the exact markdown structure `MILESTONES.md` must follow. It is the wire format for the entities in `data-model.md`. A document that parses cleanly against this contract is considered conforming; one that doesn't, isn't.

## Top-level structure

```markdown
# rustviz Milestone Roadmap

**Source of truth**: [CLAUDE.md](./CLAUDE.md)
**Last audit**: YYYY-MM-DD

> One paragraph describing what this document is and how to read it.

## Dependency graph

\`\`\`
<ascii or mermaid graph of M01 → M02 → ... edges>
\`\`\`

## Milestones

### M01 — <title>
<milestone block, see below>

### M02 — <title>
<milestone block>

…

## Deferred

- **<title>** — <reason>. (CLAUDE.md › <section> › "<quote>")
- **<title>** — <reason>.
```

Required top-level sections, in order:

1. `# rustviz Milestone Roadmap` (h1, exact text)
2. `**Source of truth**:` line
3. `**Last audit**:` line (ISO date)
4. One-paragraph intro
5. `## Dependency graph` with a code-fenced graph
6. `## Milestones` containing one h3 block per milestone
7. `## Deferred` containing the deferred bucket (may be empty bullet list — must be present)

No additional top-level sections. Anything else belongs in `notes` of a specific milestone.

## Milestone block schema

Each `### Mxx — <title>` heading is followed by a block of the form below. Field order is fixed for greppability.

```markdown
### M01 — Frontend skeleton (lexer + parser)

- **Kind**: foundation
- **Status**: planned
- **Complexity**: L (modules: 4, bullets: 3, boundaries: 0)
- **Depends on**: —
- **Authority**: CLAUDE.md › Planned code layout › "src/parse/ … span.rs, lexer.rs, ast.rs, parser.rs"; CLAUDE.md › Immediate roadmap › "Integrate the parse/ skeleton"

**Goal.** One sentence: what this milestone delivers.

**In scope.**
- bullet
- bullet
- bullet

**Out of scope.**
- bullet (often: "no name resolution", "no eval", etc.)
- bullet

**Entry criteria.**
- bullet
- bullet

**Exit criteria.**
- bullet (must mention a runnable artifact — see VR-6)
- bullet
- bullet

**Demo.**
- Format: snapshot
- Inputs: `tests/samples/m01_*.rs`
- Outputs: `tests/snapshots/m01_*.snap`
- Command: `cargo test --test m01`

**Notes.** (optional — open decisions, gotchas)
```

### Field order

Header line (h3 with id and title) → metadata bullets in the fixed order **Kind, Status, Complexity, Depends on, Authority** → labeled paragraphs in the fixed order **Goal, In scope, Out of scope, Entry criteria, Exit criteria, Demo, Notes**.

A linter / audit script walks each `### M\d{2}` block and asserts the field order verbatim. Field order is part of the contract so contributors can scan multiple milestones uniformly without re-orienting.

### Field formats

- `**Kind**:` — exactly `foundation` or `feature`.
- `**Status**:` — exactly one of `planned`, `active`, `closed`, `absorbed`, `deferred`.
- `**Complexity**:` — `S`, `M`, or `L`, followed by `(modules: N, bullets: N, boundaries: N)` showing the three sizing axes that justify the bucket. Per the rubric in `research.md` R-007. Never `XL`; XL milestones must split.
- `**Depends on**:` — comma-separated `Mxx` ids, or `—` for none.
- `**Authority**:` — semicolon-separated `CLAUDE.md › <section> › "<quote>"` citations.
- `**Demo.**` block — four labeled bullets: `Format`, `Inputs`, `Outputs`, `Command`. `Format` is `snapshot` or `browser`. `Inputs` and `Outputs` are comma-separated paths or short descriptions.

### Absorbed milestone block

A milestone with `Status: absorbed` collapses to just the redirect:

```markdown
### M06 — (absorbed)

- **Status**: absorbed → M06a, M06b

This milestone was split during implementation. See M06a and M06b.
```

No other fields required.

### Deferred milestone

Deferred items live under the top-level `## Deferred` heading, not as `### Mxx` blocks. They are one-liners:

```markdown
- **Detailed Send/Sync inference** — out of scope for a pedagogical visualizer; M08 covers happy-path Arc<Mutex<T>> only. (CLAUDE.md › Supported Rust subset › "Send/Sync")
```

## Conformance checks

The document is conforming iff all of:

1. **C-1 Structural**: Top-level section order matches the template above.
2. **C-2 IDs**: Every `### M\d{2}[a-z]?` heading has a unique id. No reuse.
3. **C-3 Field order**: In every milestone block, metadata bullets and labeled paragraphs appear in the exact order specified.
4. **C-4 Field values**: Each field parses to its declared type (Kind enum, Complexity ∈ {S, M, L} with non-XL sizing axes, etc.).
5. **C-5 DAG**: The dependency graph induced by `Depends on:` across all non-`absorbed`, non-`deferred` milestones is acyclic.
6. **C-6 Coverage**: Every scope-bullet in CLAUDE.md (see audit procedure in `quickstart.md`) is referenced by exactly one milestone's `Authority` line, or appears under `## Deferred`.
7. **C-7 Citation validity**: For every `Authority` citation, the cited section and the quoted phrase exist verbatim in the current CLAUDE.md.
8. **C-8 Demo reachability**: Every `closed` milestone has a `Demo.Command` that runs successfully from `main` at HEAD.

`C-1` through `C-5` can be checked by a shell script using `grep` + `awk`. `C-6` and `C-7` require parsing CLAUDE.md heading structure (a 20-line awk script is enough). `C-8` is enforced manually on milestone close.

## Optional audit script signature

A future companion script (not part of this feature's deliverable, but enabled by it) would have the signature:

```bash
./scripts/audit-milestones.sh
# exits 0 if document conforms to all of C-1..C-7
# exits 1 with a report listing failing checks
```

Building the script is not required by this feature — the schema is the contract; a script is one way to mechanize it but the contract stands alone.
