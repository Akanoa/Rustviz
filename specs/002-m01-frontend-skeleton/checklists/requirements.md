# Specification Quality Checklist: M01 — Frontend Skeleton

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-05-20
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- Items marked incomplete require spec updates before `/speckit.clarify` or `/speckit.plan`.
- Validation pass 1 (2026-05-20): all items pass.
- **Content Quality "no implementation details" note**: this is M01 — its job *is* to deliver implementation infrastructure (lexer, parser, AST). The spec mentions "Cargo", "snapshot tests", "recursive descent" only where these are CLAUDE.md / MILESTONES.md locked-in decisions being cited as authority, not where the spec is deciding them. The choice of snapshot library (e.g. `insta` vs hand-rolled) is explicitly deferred to plan-phase.
- **"Non-technical stakeholders"**: M01 has no non-technical stakeholders. The audience for this spec is the maintainer and the AI implementer. The criterion is interpreted as "no jargon beyond what CLAUDE.md and MILESTONES.md already establish".
- **Authority chain**: this spec defers all scope decisions to `MILESTONES.md` › M01, which itself cites `CLAUDE.md`. Re-deriving scope from CLAUDE.md is out of scope for this feature; the milestone block is the contract.

## Post-implementation audit (2026-05-20)

Following `/speckit-implement` execution of all 32 tasks (T001–T032).

### Success-criteria results

| ID    | Description | Result |
|-------|-------------|--------|
| SC-001 | `cargo test --test m01` runs ≥ 5 snapshot tests covering happy / precedence / 2 errors / empty | PASS — 8 snapshot tests pass (exceeds minimum) |
| SC-002 | 100% of AST nodes carry non-empty spans | PASS — visual review of all 8 snapshots; only `parses_empty` has a legitimate `Span(0, 0)` for the empty `Program` |
| SC-003 | Single error per failing input | PASS — `errors_on_first_of_multi` snapshot confirms exactly one error returned for an input with two |
| SC-004 | Deterministic snapshots | PASS — re-running `cargo test --test m01` produces no `.snap.new` files |
| SC-005 | ≤ ~2000 LOC under `src/parse/` | PASS — 1031 LOC across 6 files (span 117, token 132, ast 219, lexer 187, parser 354, error 22) |
| SC-006 | Zero warnings under `-D warnings` | PASS — `RUSTFLAGS="-D warnings" cargo build --release` and `cargo test --test m01` both clean |

### Implementation findings

- **Dropped `serde` dependency**: research R-004 / R-016 proposed `insta::assert_yaml_snapshot!` with `serde::Serialize` derived on AST types. During T012 (first build verify) this failed: `serde` was specified as a dev-dependency but used inside `src/` (which only sees regular dependencies). Two repair paths considered — promote `serde` to a regular dep, or switch to `assert_debug_snapshot!`. Chose the latter: no production dep, `Debug` output is plenty readable for AST snapshot review, and `insta = "1"` with default features is enough. `research.md` R-016 should be re-read with this update; future milestones can revisit serde when there's a real serialization consumer (likely M03's event stream or M04's UI bridge).
- **R-018 open question resolved**: `Type::Path { segments: Vec<String> }` + `Type::Unit` adopted as expected. No surprises during implementation. Recorded in `src/parse/ast.rs` module doc.
- **M01 path types are single-segment only**: the parser does not handle `::` in type paths because there's no `ColonColon` token kind in `TokenKind` (M01 didn't introduce one — M02 will when it actually needs to resolve paths). `parse_type` reads one identifier into `segments`. The data model allows multi-segment for forward-compatibility.
- **Unary precedence**: bound at 70 (above all binary ops, max 60 for `* / %`). Verified by `parses_full_l1` snapshot showing `-t` as `Unary { Neg, Ident "t" }`.
- **UTF-8 safety**: lexer's error span for unknown characters uses a `utf8_next_boundary` helper so a multi-byte erroneous character produces a span aligned with character boundaries, not bytes mid-character. Not exercised by any current test — should be retroactively added if a UTF-8 edge case bites.
- **`&` in comments**: confirmed by `lexer_ignores_ampersand_in_comment` snapshot — the comment-skip loop advances `pos` past the `&` byte without entering the lex-token match.

### Tasks marked done

All 32 tasks (T001–T032) in `tasks.md` were executed in the order described, with the following deviations:

- T020 (test driver) uses `insta::assert_debug_snapshot!` instead of `assert_yaml_snapshot!` per the serde change above.
- T021 / subsequent first-time snapshot runs used `INSTA_UPDATE=always cargo test --test m01` to non-interactively accept snapshots (the AI implementer has no interactive `cargo insta review` flow available). Snapshots were then visually inspected by reading the `.snap` files directly.

### Test summary

```
$ RUSTFLAGS="-D warnings" cargo test --test m01
running 8 tests
test errors_on_first_of_multi ... ok
test errors_on_unexpected_token ... ok
test lexer_ignores_ampersand_in_comment ... ok
test lexer_rejects_ampersand ... ok
test parses_arithmetic ... ok
test parses_empty ... ok
test parses_full_l1 ... ok
test parses_precedence ... ok

test result: ok. 8 passed; 0 failed; 0 ignored
```

### Conclusion

M01 exit criteria met. The crate is ready to commit. M02 (name resolution + lightweight typeck) can begin once committed.
