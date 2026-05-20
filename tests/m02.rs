//! Integration test driver for M02.
//!
//! Each sample under `tests/samples/m02_*.rs` is parsed, resolved, and
//! type-checked; the combined output is debug-snapshotted via
//! `insta::assert_debug_snapshot!`. Snapshots live under `tests/snapshots/`
//! (no module prefix).

use std::path::PathBuf;

use rustviz::{parse, resolve, typeck, ParseError, Resolution, SourceMap, TypeMap};

/// Combined output of the M02 pipeline. Holds either both side tables or the
/// first error encountered.
#[derive(Debug)]
#[allow(dead_code)] // fields read only by snapshot serialization
enum AnalyzeResult {
    Ok {
        resolution: Resolution,
        types: TypeMap,
    },
    Err(ParseError),
}

fn analyze_sample(name: &str) -> AnalyzeResult {
    let path: PathBuf = ["tests", "samples", &format!("{name}.rs")].iter().collect();
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("sample file {path:?}: {e}"));
    let mut sm = SourceMap::new();
    let file = sm.add(format!("{name}.rs"), src);

    let program = match parse(file, &sm) {
        Ok(p) => p,
        Err(e) => return AnalyzeResult::Err(e),
    };
    let resolution = match resolve(&program) {
        Ok(r) => r,
        Err(e) => return AnalyzeResult::Err(e),
    };
    let types = match typeck(&program, &resolution) {
        Ok(t) => t,
        Err(e) => return AnalyzeResult::Err(e),
    };
    AnalyzeResult::Ok { resolution, types }
}

macro_rules! sample_test {
    ($test_fn:ident, $sample:literal) => {
        #[test]
        fn $test_fn() {
            let result = analyze_sample($sample);
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

// US1 — happy path.
sample_test!(resolves_and_types_simple, "m02_simple");
sample_test!(resolves_and_types_shadow, "m02_shadow");
sample_test!(resolves_and_types_fn_params, "m02_fn_params");
sample_test!(resolves_and_types_if_expr, "m02_if_expr");

// US2 — resolver errors.
sample_test!(errors_on_undeclared, "m02_undeclared");
sample_test!(errors_on_first_undeclared, "m02_undeclared_first");
sample_test!(errors_on_duplicate_param, "m02_dup_param");

// US3 — typeck errors.
sample_test!(errors_on_annotation_mismatch, "m02_type_mismatch");
sample_test!(errors_on_op_mismatch, "m02_op_mismatch");
sample_test!(errors_on_if_cond, "m02_if_cond");
sample_test!(errors_on_if_branch_mismatch, "m02_if_branch");
sample_test!(errors_on_return_mismatch, "m02_ret_mismatch");
sample_test!(errors_on_call_arity, "m02_call_arity");
sample_test!(errors_on_arg_mismatch, "m02_arg_mismatch");
sample_test!(errors_on_non_fn_call, "m02_non_fn_call");
sample_test!(errors_on_non_ident_callee, "m02_non_ident_callee");
