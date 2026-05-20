//! Integration test driver for M01.
//!
//! Each sample under `tests/samples/m01_*.rs` is parsed and the `Result` is
//! debug-snapshotted via `insta::assert_debug_snapshot!`. Snapshots live under
//! `tests/snapshots/` (no module prefix).

use std::path::PathBuf;

use rustviz::{parse, ParseError, SourceMap};

fn parse_sample(name: &str) -> Result<rustviz::ast::Program, ParseError> {
    let path: PathBuf = ["tests", "samples", &format!("{name}.rs")].iter().collect();
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("sample file {path:?}: {e}"));
    let mut sm = SourceMap::new();
    let file = sm.add(format!("{name}.rs"), src);
    parse(file, &sm)
}

macro_rules! sample_test {
    ($test_fn:ident, $sample:literal) => {
        #[test]
        fn $test_fn() {
            let result = parse_sample($sample);
            insta::with_settings!(
                {
                    snapshot_path => "snapshots",
                    prepend_module_to_snapshot => false,
                },
                {
                    insta::assert_debug_snapshot!(result);
                }
            );
        }
    };
}

// US1 — happy path + edge cases.
sample_test!(parses_arithmetic, "m01_arithmetic");
sample_test!(parses_precedence, "m01_precedence");
sample_test!(parses_full_l1, "m01_full_l1");
sample_test!(parses_empty, "m01_empty");

// US2 — span-bearing errors.
sample_test!(errors_on_unexpected_token, "m01_unexpected_token");
sample_test!(errors_on_first_of_multi, "m01_multi_error");

// US3 — lexer rejects `&` with a "Level 2" pedagogical message.
sample_test!(lexer_rejects_ampersand, "m01_reject_ampersand");
sample_test!(lexer_ignores_ampersand_in_comment, "m01_ampersand_in_comment");
