use logic_lens_core::config::Config;
use logic_lens_core::diff::match_entities;
use logic_lens_core::entity::extract_entities;
use logic_lens_core::hallucination::check_hallucinated_imports;
use logic_lens_core::lint::{run_builtin_lenses, AuditLens, ChangeContext};
use logic_lens_core::output::{render_json, AuditResult};
use logic_lens_core::parser::parse_file;
use logic_lens_core::rules::load_all_rules;
use logic_lens_core::verify::run_verify;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let error_resp = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    }
                });
                let _ = writeln!(stdout, "{}", error_resp);
                let _ = stdout.flush();
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        let response = match method {
            "initialize" => handle_initialize(&id),
            "tools/list" => handle_tools_list(&id),
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or(Value::Null);
                handle_tools_call(&id, &params)
            }
            "shutdown" | "notifications/cancelled" => {
                // Graceful shutdown
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {}
                });
                let _ = writeln!(stdout, "{}", resp);
                let _ = stdout.flush();
                break;
            }
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {}", method)
                }
            }),
        };

        let _ = writeln!(stdout, "{}", response);
        let _ = stdout.flush();
    }
}

fn handle_initialize(id: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "logic-lens-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

fn handle_tools_list(id: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": [
                {
                    "name": "ll_audit",
                    "description": "Run full semantic audit comparing an old and new file. Returns structured JSON with entities, changes, classifications, findings, and metadata.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "old_file": {
                                "type": "string",
                                "description": "Absolute path to the original file"
                            },
                            "new_file": {
                                "type": "string",
                                "description": "Absolute path to the new/modified file"
                            }
                        },
                        "required": ["old_file", "new_file"]
                    }
                },
                {
                    "name": "ll_findings",
                    "description": "Run audit and return only findings (lint warnings, hallucination alerts, rule violations) without full diff data.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "old_file": {
                                "type": "string",
                                "description": "Absolute path to the original file"
                            },
                            "new_file": {
                                "type": "string",
                                "description": "Absolute path to the new/modified file"
                            }
                        },
                        "required": ["old_file", "new_file"]
                    }
                },
                {
                    "name": "ll_verify",
                    "description": "Run local build/test verification for a project directory. Detects build tool and runs the appropriate command.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "project_dir": {
                                "type": "string",
                                "description": "Absolute path to the project directory"
                            },
                            "command": {
                                "type": "string",
                                "description": "Optional custom build command override"
                            },
                            "timeout_secs": {
                                "type": "integer",
                                "description": "Timeout in seconds (default: 120)"
                            }
                        },
                        "required": ["project_dir"]
                    }
                }
            ]
        }
    })
}

fn handle_tools_call(id: &Value, params: &Value) -> Value {
    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    match tool_name {
        "ll_audit" => handle_ll_audit(id, &arguments),
        "ll_findings" => handle_ll_findings(id, &arguments),
        "ll_verify" => handle_ll_verify(id, &arguments),
        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32602,
                "message": format!("Unknown tool: {}", tool_name)
            }
        }),
    }
}

fn run_audit(old_file: &str, new_file: &str) -> Result<AuditResult, String> {
    let old_path = PathBuf::from(old_file);
    let new_path = PathBuf::from(new_file);

    if !old_path.exists() {
        return Err(format!("Old file not found: {}", old_file));
    }
    if !new_path.exists() {
        return Err(format!("New file not found: {}", new_file));
    }

    let total_start = Instant::now();

    let parse_start = Instant::now();
    let old_parsed = parse_file(&old_path).map_err(|e| format!("Parse error (old): {}", e))?;
    let new_parsed = parse_file(&new_path).map_err(|e| format!("Parse error (new): {}", e))?;

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
    let config = Config::discover(&new_path);
    let ctx = ChangeContext {
        old_source: &old_parsed.source,
        new_source: &new_parsed.source,
        old_entities: &old_entities,
        new_entities: &new_entities,
        change_set: &change_set,
        language: new_parsed.language,
        new_file_path: new_file,
    };

    let mut findings = run_builtin_lenses(&ctx, &config);

    let rules_dir = config.rules_dir.clone().unwrap_or_else(|| {
        new_path
            .parent()
            .unwrap_or(&new_path)
            .join(".logic-lens/rules")
    });
    let (yaml_lenses, _) = load_all_rules(&rules_dir);
    for lens in &yaml_lenses {
        findings.extend(lens.evaluate(&ctx));
    }

    let hallucination =
        check_hallucinated_imports(&new_parsed.source, new_parsed.language, &new_path);
    findings.extend(hallucination.findings);

    let analyze_duration = analyze_start.elapsed();
    let total_duration = total_start.elapsed();

    Ok(AuditResult::build(
        old_file,
        new_file,
        &format!("{:?}", new_parsed.language),
        &old_entities,
        &new_entities,
        &change_set,
        findings,
        parse_duration,
        diff_duration,
        analyze_duration,
        total_duration,
    ))
}

fn handle_ll_audit(id: &Value, args: &Value) -> Value {
    let old_file = args.get("old_file").and_then(|v| v.as_str()).unwrap_or("");
    let new_file = args.get("new_file").and_then(|v| v.as_str()).unwrap_or("");

    if old_file.is_empty() || new_file.is_empty() {
        return mcp_error(
            id,
            -32602,
            "Missing required parameters: old_file, new_file",
        );
    }

    match run_audit(old_file, new_file) {
        Ok(result) => {
            let json_str = render_json(&result);
            let content: Value = serde_json::from_str(&json_str).unwrap_or(Value::Null);
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string(&content).unwrap_or_default()
                    }]
                }
            })
        }
        Err(e) => mcp_error(id, -32000, &e),
    }
}

fn handle_ll_findings(id: &Value, args: &Value) -> Value {
    let old_file = args.get("old_file").and_then(|v| v.as_str()).unwrap_or("");
    let new_file = args.get("new_file").and_then(|v| v.as_str()).unwrap_or("");

    if old_file.is_empty() || new_file.is_empty() {
        return mcp_error(
            id,
            -32602,
            "Missing required parameters: old_file, new_file",
        );
    }

    match run_audit(old_file, new_file) {
        Ok(result) => {
            let findings_json = json!({
                "findings": result.findings,
                "count": result.findings.len(),
                "has_errors": result.has_errors(),
            });
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&findings_json).unwrap_or_default()
                    }]
                }
            })
        }
        Err(e) => mcp_error(id, -32000, &e),
    }
}

fn handle_ll_verify(id: &Value, args: &Value) -> Value {
    let project_dir = args
        .get("project_dir")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let command = args.get("command").and_then(|v| v.as_str());
    let timeout_secs = args
        .get("timeout_secs")
        .and_then(|v| v.as_u64())
        .unwrap_or(120);

    if project_dir.is_empty() {
        return mcp_error(id, -32602, "Missing required parameter: project_dir");
    }

    let dir = PathBuf::from(project_dir);
    if !dir.exists() {
        return mcp_error(id, -32000, &format!("Directory not found: {}", project_dir));
    }

    let result = run_verify(command, &dir, Duration::from_secs(timeout_secs));
    let result_json = serde_json::to_string_pretty(&result).unwrap_or_default();

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{
                "type": "text",
                "text": result_json
            }]
        }
    })
}

fn mcp_error(id: &Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_initialize() {
        let resp = handle_initialize(&json!(1));
        assert_eq!(resp["result"]["serverInfo"]["name"], "logic-lens-mcp");
    }

    #[test]
    fn test_tools_list() {
        let resp = handle_tools_list(&json!(2));
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 3);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"ll_audit"));
        assert!(names.contains(&"ll_findings"));
        assert!(names.contains(&"ll_verify"));
    }

    #[test]
    fn test_ll_audit_missing_file() {
        let args = json!({"old_file": "/nonexistent/old.ts", "new_file": "/nonexistent/new.ts"});
        let resp = handle_ll_audit(&json!(3), &args);
        assert!(resp.get("error").is_some());
        assert!(resp["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found"));
    }

    #[test]
    fn test_ll_audit_with_real_files() {
        let dir = TempDir::new().unwrap();
        let old_path = dir.path().join("old.ts");
        let new_path = dir.path().join("new.ts");

        std::fs::write(&old_path, "function foo() { return 1; }\n").unwrap();
        std::fs::write(&new_path, "function foo() { return 2; }\n").unwrap();

        let args = json!({
            "old_file": old_path.to_str().unwrap(),
            "new_file": new_path.to_str().unwrap()
        });
        let resp = handle_ll_audit(&json!(4), &args);
        assert!(resp.get("result").is_some());
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let audit: Value = serde_json::from_str(text).unwrap();
        assert!(audit.get("changes").is_some());
    }

    #[test]
    fn test_ll_findings_with_real_files() {
        let dir = TempDir::new().unwrap();
        let old_path = dir.path().join("old.ts");
        let new_path = dir.path().join("new.ts");

        std::fs::write(&old_path, "function foo() { return 1; }\n").unwrap();
        std::fs::write(&new_path, "function foo() {\n    // TODO: implement\n}\n").unwrap();

        let args = json!({
            "old_file": old_path.to_str().unwrap(),
            "new_file": new_path.to_str().unwrap()
        });
        let resp = handle_ll_findings(&json!(5), &args);
        assert!(resp.get("result").is_some());
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let findings: Value = serde_json::from_str(text).unwrap();
        assert!(findings["count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn test_ll_verify_no_project() {
        let dir = TempDir::new().unwrap();
        let args = json!({"project_dir": dir.path().to_str().unwrap()});
        let resp = handle_ll_verify(&json!(6), &args);
        assert!(resp.get("result").is_some());
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let verify: Value = serde_json::from_str(text).unwrap();
        assert_eq!(verify["success"], false);
    }

    #[test]
    fn test_unknown_tool() {
        let params = json!({"name": "nonexistent", "arguments": {}});
        let resp = handle_tools_call(&json!(7), &params);
        assert!(resp.get("error").is_some());
    }

    #[test]
    fn test_missing_params() {
        let args = json!({});
        let resp = handle_ll_audit(&json!(8), &args);
        assert!(resp.get("error").is_some());
    }
}
