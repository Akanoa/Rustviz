//! Trace generator binary — runs the M01→M02→M03 pipeline on each sample
//! under `web/samples/` and writes a JSON trace file under `web/traces/`.
//!
//! Trace JSON shape (matches `specs/005-m04-ui-shell/contracts/m04-api.md`):
//!
//! ```json
//! { "source": "<rs text>", "events": [<MemEvent>, ...] }
//! ```
//!
//! Invocation: `cargo run --release --bin gen_traces`. Trunk runs this as
//! a pre-build hook (see `Trunk.toml`).

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::json;

use rustviz::run_pipeline;

/// Samples to process. Each name `X` is read from `web/samples/X.rs` and
/// written to `web/traces/X.json`.
const SAMPLES: &[&str] = &[
    "m03_arithmetic",
    "m03_fn_call",
    "m03_fn_call_twice",
    "m03_shadow",
    "m03_div_by_zero",
];

fn main() -> ExitCode {
    // Anchor to the project root so paths work regardless of CWD (cargo run
    // from repo root, trunk pre-build hook from `web/`, etc.).
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let samples_dir = project_root.join("web/samples");
    let traces_dir = project_root.join("web/traces");

    if let Err(e) = fs::create_dir_all(&traces_dir) {
        eprintln!("gen_traces: cannot create {traces_dir:?}: {e}");
        return ExitCode::FAILURE;
    }

    let mut failures = 0u32;
    for sample in SAMPLES {
        match process_one(sample, &samples_dir, &traces_dir) {
            Ok(event_count) => {
                println!("gen_traces: wrote {sample}.json (events: {event_count})");
            }
            Err(e) => {
                eprintln!("gen_traces: sample {sample} failed: {e}");
                failures += 1;
            }
        }
    }

    if failures > 0 {
        eprintln!("gen_traces: {failures} sample(s) failed");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn process_one(
    sample: &str,
    samples_dir: &PathBuf,
    traces_dir: &PathBuf,
) -> Result<usize, String> {
    let source_path = samples_dir.join(format!("{sample}.rs"));
    let trace_path = traces_dir.join(format!("{sample}.json"));

    let source = fs::read_to_string(&source_path)
        .map_err(|e| format!("read {source_path:?}: {e}"))?;

    // M05: use the consolidated pipeline runner. Same four stages as before,
    // wrapped in a single error type that carries the failing stage label.
    let events = run_pipeline(&source).map_err(|e| {
        format!("{:?} error: {} (at {:?})", e.stage, e.message, e.span)
    })?;

    let event_count = events.len();
    let doc = json!({
        "source": source,
        "events": events,
    });
    let serialized = serde_json::to_string_pretty(&doc)
        .map_err(|e| format!("serialize: {e}"))?;
    fs::write(&trace_path, serialized)
        .map_err(|e| format!("write {trace_path:?}: {e}"))?;

    Ok(event_count)
}
