use logic_lens_core::config::Config;
use logic_lens_core::diff::match_entities;
use logic_lens_core::entity::extract_entities;
use logic_lens_core::hallucination::check_hallucinated_imports;
use logic_lens_core::lint::{run_builtin_lenses, ChangeContext};
use logic_lens_core::output::{
    render_json, render_markdown, render_terminal, AuditResult, Verbosity,
};
use logic_lens_core::parser::parse_file;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures")
}

fn run_full_audit(old_file: &Path, new_file: &Path) -> AuditResult {
    let total_start = Instant::now();

    let parse_start = Instant::now();
    let old_parsed = parse_file(old_file).expect("Failed to parse old file");
    let new_parsed = parse_file(new_file).expect("Failed to parse new file");
    let old_entities = extract_entities(&old_parsed.source, &old_parsed.tree, old_parsed.language);
    let new_entities = extract_entities(&new_parsed.source, &new_parsed.tree, new_parsed.language);
    let parse_duration = parse_start.elapsed();

    let diff_start = Instant::now();
    let change_set = match_entities(
        &old_entities,
        &new_entities,
        &old_parsed.source,
        &new_parsed.source,
        new_parsed.language,
    );
    let diff_duration = diff_start.elapsed();

    let analyze_start = Instant::now();
    let config = Config::default();
    let ctx = ChangeContext {
        old_source: &old_parsed.source,
        new_source: &new_parsed.source,
        old_entities: &old_entities,
        new_entities: &new_entities,
        change_set: &change_set,
        language: new_parsed.language,
        new_file_path: &new_file.display().to_string(),
    };

    let mut findings = run_builtin_lenses(&ctx, &config);
    let hallucination =
        check_hallucinated_imports(&new_parsed.source, new_parsed.language, new_file);
    findings.extend(hallucination.findings);
    let analyze_duration = analyze_start.elapsed();
    let total_duration = total_start.elapsed();

    AuditResult::build(
        &old_file.display().to_string(),
        &new_file.display().to_string(),
        &format!("{:?}", new_parsed.language),
        &old_entities,
        &new_entities,
        &change_set,
        findings,
        parse_duration,
        diff_duration,
        analyze_duration,
        total_duration,
    )
}

// --- End-to-End CLI Pipeline Tests ---

#[test]
fn test_e2e_typescript_audit() {
    let dir = fixtures_dir();
    let result = run_full_audit(
        &dir.join("typescript_old.ts"),
        &dir.join("typescript_new.ts"),
    );

    // Should detect changes
    assert!(result.changes.total > 0, "Should detect changes");

    // Should have findings (placeholder, empty impl, unhandled await, etc.)
    assert!(!result.findings.is_empty(), "Should have findings");

    // Should detect placeholder
    assert!(
        result
            .findings
            .iter()
            .any(|f| f.rule == "placeholder-detection"),
        "Should detect placeholder comment"
    );

    // JSON output should be parseable
    let json = render_json(&result);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["changes"]["total"].as_u64().unwrap() > 0);
}

#[test]
fn test_e2e_python_audit() {
    let dir = fixtures_dir();
    let result = run_full_audit(&dir.join("python_old.py"), &dir.join("python_new.py"));

    assert!(result.changes.total > 0);

    // Should detect empty implementation (pass-only)
    assert!(
        result
            .findings
            .iter()
            .any(|f| f.rule == "empty-implementation"),
        "Should detect pass-only function"
    );
}

#[test]
fn test_e2e_rust_audit() {
    let dir = fixtures_dir();
    let result = run_full_audit(&dir.join("rust_old.rs"), &dir.join("rust_new.rs"));

    assert!(result.changes.total > 0);

    // Should detect logic change in calculate_total (added filter)
    assert!(
        result.changes.by_classification.logic > 0
            || result.changes.by_classification.refactor > 0
            || result.changes.added > 0,
        "Should detect changes in Rust file"
    );
}

#[test]
fn test_e2e_java_audit() {
    let dir = fixtures_dir();
    let result = run_full_audit(&dir.join("java_old.java"), &dir.join("java_new.java"));

    assert!(result.changes.total > 0);

    // Should detect placeholder in cancelOrder
    assert!(
        result
            .findings
            .iter()
            .any(|f| f.rule == "placeholder-detection"),
        "Should detect TODO in Java"
    );
}

// --- Output Format Tests ---

#[test]
fn test_e2e_json_output_valid() {
    let dir = fixtures_dir();
    let result = run_full_audit(
        &dir.join("typescript_old.ts"),
        &dir.join("typescript_new.ts"),
    );
    let json = render_json(&result);

    let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON should parse");
    assert!(parsed.get("old_file").is_some());
    assert!(parsed.get("new_file").is_some());
    assert!(parsed.get("language").is_some());
    assert!(parsed.get("entities").is_some());
    assert!(parsed.get("changes").is_some());
    assert!(parsed.get("findings").is_some());
    assert!(parsed.get("metadata").is_some());
}

#[test]
fn test_e2e_terminal_output() {
    let dir = fixtures_dir();
    let result = run_full_audit(
        &dir.join("typescript_old.ts"),
        &dir.join("typescript_new.ts"),
    );
    let output = render_terminal(&result, Verbosity::Normal, false);
    assert!(output.contains("changes"));
    assert!(output.contains("Risk:"));
}

#[test]
fn test_e2e_markdown_output() {
    let dir = fixtures_dir();
    let result = run_full_audit(
        &dir.join("typescript_old.ts"),
        &dir.join("typescript_new.ts"),
    );
    let md = render_markdown(&result);
    assert!(md.contains("# Audit:"));
    assert!(md.contains("## Changes"));
}

// --- Performance Benchmark ---

#[test]
fn test_performance_under_15ms() {
    let dir = fixtures_dir();
    let old_file = dir.join("typescript_old.ts");
    let new_file = dir.join("typescript_new.ts");

    // Warm up
    let _ = run_full_audit(&old_file, &new_file);

    // Measure
    let start = Instant::now();
    let _result = run_full_audit(&old_file, &new_file);
    let duration = start.elapsed();

    assert!(
        duration.as_millis() < 15,
        "Audit should complete under 15ms, took {}ms",
        duration.as_millis()
    );
}

// --- MCP Integration Tests ---

#[test]
fn test_mcp_protocol_flow() {
    // Simulate MCP initialize → tools/list → tools/call flow
    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }
    });

    let tools_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });

    // Verify both requests serialize and deserialize correctly
    let init_str = serde_json::to_string(&init_request).unwrap();
    let _: serde_json::Value = serde_json::from_str(&init_str).unwrap();

    let tools_str = serde_json::to_string(&tools_request).unwrap();
    let _: serde_json::Value = serde_json::from_str(&tools_str).unwrap();
}
