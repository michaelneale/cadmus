// ---------------------------------------------------------------------------
// Integration tests for the agent tools pipeline
// ---------------------------------------------------------------------------
//
// Tests the full chain: tool_definitions → tool_executor → result.
// These tests do NOT require an LLM server — they test the Cadmus side
// of the integration (tool catalog generation and tool execution).
//
// Tests that actually execute ops require Racket to be installed.

use std::collections::HashMap;

use cadmus::tools;
use cadmus::tool_executor;

// ---------------------------------------------------------------------------
// Tool catalog tests
// ---------------------------------------------------------------------------

#[test]
fn test_tool_catalog_has_code_editing_ops() {
    let tools = tools::tool_definitions(false);
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t["function"]["name"].as_str())
        .collect();

    // All core code editing ops should be present
    assert!(names.contains(&"grep_code"), "missing grep_code");
    assert!(names.contains(&"find_definition"), "missing find_definition");
    assert!(names.contains(&"find_usages"), "missing find_usages");
    assert!(names.contains(&"file_outline"), "missing file_outline");
    assert!(names.contains(&"list_source_files"), "missing list_source_files");
    assert!(names.contains(&"build_project"), "missing build_project");
    assert!(names.contains(&"test_project"), "missing test_project");
}

#[test]
fn test_tool_catalog_has_build_ops() {
    let tools = tools::tool_definitions(false);
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t["function"]["name"].as_str())
        .collect();

    assert!(names.contains(&"build_project"), "missing build_project");
    assert!(names.contains(&"test_project"), "missing test_project");
    assert!(names.contains(&"lint_project"), "missing lint_project");
}

#[test]
fn test_tool_definitions_valid_json_schema() {
    let tools = tools::tool_definitions(false);
    for tool in &tools {
        // Each tool should be valid OpenAI function tool format
        assert_eq!(tool["type"], "function", "tool type should be 'function'");

        let func = &tool["function"];
        assert!(func["name"].is_string(), "function.name should be string");
        assert!(func["description"].is_string(), "function.description should be string");

        let params = &func["parameters"];
        assert_eq!(params["type"], "object", "parameters.type should be 'object'");
        assert!(params["properties"].is_object(), "parameters.properties should be object");
        assert!(params["required"].is_array(), "parameters.required should be array");

        // Required params should be a subset of properties keys
        let prop_keys: Vec<&str> = params["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        for req in params["required"].as_array().unwrap() {
            let req_name = req.as_str().unwrap();
            assert!(
                prop_keys.contains(&req_name),
                "required param '{}' not in properties for op '{}'",
                req_name,
                func["name"]
            );
        }
    }
}

#[test]
fn test_tool_definitions_round_trip_to_json_string() {
    let tools = tools::tool_definitions(false);
    let json_str = serde_json::to_string(&tools).expect("should serialize to JSON");
    let parsed: Vec<serde_json::Value> =
        serde_json::from_str(&json_str).expect("should parse back");
    assert_eq!(parsed.len(), tools.len());
}

// ---------------------------------------------------------------------------
// Tool execution tests (require Racket)
// ---------------------------------------------------------------------------

fn has_racket() -> bool {
    std::process::Command::new("racket")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn test_execute_grep_code_finds_main() {
    if !has_racket() {
        eprintln!("skipping: racket not installed");
        return;
    }

    let mut args = HashMap::new();
    args.insert("dir".into(), ".".into());
    args.insert("pattern".into(), "fn main".into());
    let result = tool_executor::execute_tool("grep_code", &args, false);
    assert!(result.success, "grep_code should succeed: {}", result.output);
    assert!(
        result.output.contains("main"),
        "should find 'fn main' in output: {}",
        result.output
    );
}

#[test]
fn test_execute_find_definition() {
    if !has_racket() {
        eprintln!("skipping: racket not installed");
        return;
    }

    let mut args = HashMap::new();
    args.insert("dir".into(), ".".into());
    args.insert("name".into(), "compile_plan".into());
    let result = tool_executor::execute_tool("find_definition", &args, false);
    assert!(result.success, "find_definition should succeed: {}", result.output);
    assert!(
        result.output.contains("plan.rs"),
        "should find compile_plan in plan.rs: {}",
        result.output
    );
}

#[test]
fn test_execute_find_usages() {
    if !has_racket() {
        eprintln!("skipping: racket not installed");
        return;
    }

    let mut args = HashMap::new();
    args.insert("dir".into(), ".".into());
    args.insert("symbol".into(), "PlanDef".into());
    let result = tool_executor::execute_tool("find_usages", &args, false);
    assert!(result.success, "find_usages should succeed: {}", result.output);
    assert!(
        result.output.contains("plan.rs") || result.output.contains("PlanDef"),
        "should find PlanDef usages: {}",
        result.output
    );
}

#[test]
fn test_execute_list_source_files() {
    if !has_racket() {
        eprintln!("skipping: racket not installed");
        return;
    }

    let mut args = HashMap::new();
    args.insert("dir".into(), ".".into());
    let result = tool_executor::execute_tool("list_source_files", &args, false);
    assert!(result.success, "list_source_files should succeed: {}", result.output);
    assert!(
        result.output.contains(".rs"),
        "should find .rs files: {}",
        result.output
    );
}

#[test]
fn test_execute_file_outline() {
    if !has_racket() {
        eprintln!("skipping: racket not installed");
        return;
    }

    let mut args = HashMap::new();
    args.insert("file".into(), "src/tools.rs".into());
    let result = tool_executor::execute_tool("file_outline", &args, false);
    assert!(result.success, "file_outline should succeed: {}", result.output);
    assert!(
        result.output.contains("fn ") || result.output.contains("tool_definitions"),
        "should show function defs: {}",
        result.output
    );
}

// ---------------------------------------------------------------------------
// Tool execution round-trip test (simulates what the agent loop does)
// ---------------------------------------------------------------------------

#[test]
fn test_agent_simulation_grep_then_find_def() {
    if !has_racket() {
        eprintln!("skipping: racket not installed");
        return;
    }

    // Step 1: grep for a function
    let mut args1 = HashMap::new();
    args1.insert("dir".into(), ".".into());
    args1.insert("pattern".into(), "pub fn build_full_registry".into());
    let result1 = tool_executor::execute_tool("grep_code", &args1, false);
    assert!(result1.success, "step 1 grep should succeed: {}", result1.output);
    assert!(result1.output.contains("fs_types"), "should find fs_types.rs");

    // Step 2: find the definition based on what we learned
    let mut args2 = HashMap::new();
    args2.insert("dir".into(), "src".into());
    args2.insert("name".into(), "build_full_registry".into());
    let result2 = tool_executor::execute_tool("find_definition", &args2, false);
    assert!(result2.success, "step 2 find_definition should succeed: {}", result2.output);
    assert!(
        result2.output.contains("fs_types"),
        "should confirm build_full_registry is in fs_types: {}",
        result2.output
    );
}

// ---------------------------------------------------------------------------
// Safety tests
// ---------------------------------------------------------------------------

#[test]
fn test_read_only_blocks_all_write_ops() {
    let write_ops = ["sed_replace", "fix_import", "add_after", "remove_lines", "fix_assertion"];
    for op in &write_ops {
        let result = tool_executor::execute_tool(op, &HashMap::new(), true);
        assert!(!result.success, "{} should be blocked in read-only mode", op);
        assert!(
            result.output.contains("write op") || result.output.contains("read-only"),
            "{} error should mention write/read-only: {}",
            op,
            result.output
        );
    }
}

#[test]
fn test_unknown_op_gives_clear_error() {
    let result = tool_executor::execute_tool("nonexistent_op_12345", &HashMap::new(), false);
    assert!(!result.success);
    assert!(
        result.output.contains("Unknown"),
        "should say unknown op: {}",
        result.output
    );
}

// ---------------------------------------------------------------------------
// Catalog consistency tests
// ---------------------------------------------------------------------------

#[test]
fn test_all_catalog_ops_are_executable() {
    // Every op in the catalog should at least compile (produce a script)
    // even if execution requires racket
    let ops = tools::available_ops(false);
    let reg = cadmus::fs_types::build_full_registry();

    for op_name in &ops {
        let entry = reg.get_poly(op_name);
        assert!(
            entry.is_some(),
            "catalog op '{}' should be in registry",
            op_name
        );
    }
}

// ---------------------------------------------------------------------------
// Synthetic tool integration tests
// ---------------------------------------------------------------------------

#[test]
fn test_write_and_read_file_round_trip() {
    let tmp = std::env::temp_dir().join("cadmus_agent_test_roundtrip.txt");
    let path = tmp.to_string_lossy().to_string();

    // Write
    let mut write_args = HashMap::new();
    write_args.insert("path".into(), path.clone());
    write_args.insert("content".into(), "Hello from agent test!".into());
    let w = tool_executor::execute_tool("write_file", &write_args, false);
    assert!(w.success, "write should succeed: {}", w.output);

    // Read back
    let mut read_args = HashMap::new();
    read_args.insert("path".into(), path.clone());
    let r = tool_executor::execute_tool("read_file", &read_args, false);
    assert!(r.success, "read should succeed: {}", r.output);
    assert!(r.output.contains("Hello from agent test!"));

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_shell_tool_runs_command() {
    let mut args = HashMap::new();
    args.insert("command".into(), "echo cadmus_shell_test && date +%s".into());
    let result = tool_executor::execute_tool("shell", &args, false);
    assert!(result.success, "shell should succeed: {}", result.output);
    assert!(result.output.contains("cadmus_shell_test"));
}

#[test]
fn test_write_html_file_scenario() {
    // Simulates the "write pacman game" scenario:
    // 1. LLM generates HTML content (we simulate with a string)
    // 2. write_file tool writes it
    // 3. We verify the file exists and contains valid HTML

    let tmp = std::env::temp_dir().join("cadmus_agent_test_game.html");
    let path = tmp.to_string_lossy().to_string();

    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test Game</title></head>
<body>
<canvas id="game" width="400" height="400"></canvas>
<script>
const canvas = document.getElementById('game');
const ctx = canvas.getContext('2d');
ctx.fillStyle = 'yellow';
ctx.beginPath();
ctx.arc(200, 200, 50, 0.2 * Math.PI, 1.8 * Math.PI);
ctx.lineTo(200, 200);
ctx.fill();
</script>
</body>
</html>"#;

    let mut args = HashMap::new();
    args.insert("path".into(), path.clone());
    args.insert("content".into(), html.into());
    let result = tool_executor::execute_tool("write_file", &args, false);
    assert!(result.success, "write_file should succeed: {}", result.output);

    // Verify file
    let content = std::fs::read_to_string(&tmp).unwrap();
    assert!(content.contains("<!DOCTYPE html>"));
    assert!(content.contains("canvas"));
    assert!(content.contains("pacman") || content.contains("game") || content.contains("Game"));

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_synthetic_tools_in_catalog() {
    let tools = tools::tool_definitions(false);
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t["function"]["name"].as_str())
        .collect();

    assert!(names.contains(&"write_file"), "should include write_file");
    assert!(names.contains(&"read_file"), "should include read_file");
    assert!(names.contains(&"shell"), "should include shell");
}

#[test]
fn test_synthetic_tools_excluded_in_read_only() {
    let tools = tools::tool_definitions(true);
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t["function"]["name"].as_str())
        .collect();

    assert!(!names.contains(&"write_file"), "write_file should be excluded in read-only");
    assert!(!names.contains(&"shell"), "shell should be excluded in read-only");
    assert!(names.contains(&"read_file"), "read_file should still be available");
}

// ---------------------------------------------------------------------------
// Catalog consistency tests
// ---------------------------------------------------------------------------

#[test]
fn test_all_catalog_ops_have_input_names() {
    // Every op exposed to the agent must have named parameters
    // (otherwise the LLM can't construct tool calls)
    let reg = cadmus::fs_types::build_full_registry();
    let ops = tools::available_ops(false);

    for op_name in &ops {
        if let Some(entry) = reg.get_poly(op_name) {
            // Ops with inputs should have input_names
            if !entry.signature.inputs.is_empty() {
                assert!(
                    !entry.input_names.is_empty(),
                    "op '{}' has inputs but no input_names",
                    op_name
                );
                assert_eq!(
                    entry.input_names.len(),
                    entry.signature.inputs.len(),
                    "op '{}' input_names count mismatch: {:?} vs {:?}",
                    op_name,
                    entry.input_names,
                    entry.signature.inputs
                );
            }
        }
    }
}
