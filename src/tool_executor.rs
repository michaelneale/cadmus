// ---------------------------------------------------------------------------
// tool_executor — execute individual Cadmus ops as tool calls
// ---------------------------------------------------------------------------
//
// Bridge between an LLM agent's tool calls and Cadmus's typed pipeline.
// The LLM says `grep_code(dir=".", pattern="async")` and gets back the
// actual grep output. The executor:
//
//   1. Validates the op exists in the registry
//   2. Builds a minimal PlanDef (one step + bindings)
//   3. Compiles, codegens, and executes through the standard pipeline
//   4. Returns stdout/stderr as a string
//
// The LLM never sees Racket code, sexpr plans, or shell commands.

use std::collections::HashMap;

use crate::calling_frame::{CallingFrame, DefaultFrame, InvokeError};
use crate::plan::{PlanDef, PlanInput, RawStep, StepArgs};

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

/// The result of executing a single tool call.
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Whether the execution succeeded (exit code 0).
    pub success: bool,
    /// The output text (stdout on success, or combined error info on failure).
    pub output: String,
    /// The generated Racket script (for debugging/logging).
    pub script: Option<String>,
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

/// Maximum output length returned to the LLM. Longer outputs are truncated
/// to avoid blowing the context window.
const MAX_OUTPUT_CHARS: usize = 4000;

/// Execute a single tool call by op name and argument map.
///
/// Returns a `ToolResult` with the execution output. The output is
/// truncated to `MAX_OUTPUT_CHARS` to stay within LLM context limits.
///
/// # Arguments
/// * `op_name` — the operation name (must be in the registry)
/// * `args` — parameter name → value map (e.g., `{"dir": ".", "pattern": "TODO"}`)
/// * `read_only` — if true, rejects write ops
///
/// # Examples
/// ```ignore
/// let mut args = HashMap::new();
/// args.insert("dir".into(), ".".into());
/// args.insert("pattern".into(), "TODO".into());
/// let result = execute_tool("grep_code", &args, false);
/// assert!(result.success);
/// ```
pub fn execute_tool(
    op_name: &str,
    args: &HashMap<String, String>,
    read_only: bool,
) -> ToolResult {
    // Safety check: reject write ops in read-only mode
    if read_only && crate::tools::is_write_op(op_name) {
        return ToolResult {
            success: false,
            output: format!(
                "Operation '{}' is a write op and cannot be used in read-only mode.",
                op_name
            ),
            script: None,
        };
    }

    // Handle synthetic tools (not in the Cadmus registry)
    if crate::tools::is_synthetic(op_name) {
        return execute_synthetic(op_name, args);
    }

    let reg = crate::fs_types::build_full_registry();

    // Validate op exists
    let entry = match reg.get_poly(op_name) {
        Some(e) => e,
        None => {
            return ToolResult {
                success: false,
                output: format!("Unknown operation: '{}'", op_name),
                script: None,
            };
        }
    };

    // Build a minimal PlanDef: one step, bindings from args.
    // We must provide explicit type hints from the op signature so the
    // plan compiler doesn't infer wrong types from parameter names
    // (e.g., "dir" → Dir(Bytes) when the op expects String).
    let inputs: Vec<PlanInput> = entry
        .input_names
        .iter()
        .zip(entry.signature.inputs.iter())
        .map(|(name, ty)| PlanInput::typed(name, ty.to_string()))
        .collect();

    let plan = PlanDef {
        name: format!("agent-{}", op_name),
        inputs,
        output: None,
        steps: vec![RawStep {
            op: op_name.to_string(),
            args: StepArgs::None,
        }],
        bindings: args.clone(),
    };

    // Compile + codegen + execute through the existing pipeline
    let frame = DefaultFrame::from_plan(&plan);

    // First try codegen to capture the script for debugging
    let script = match frame.codegen(&plan) {
        Ok(s) => Some(s),
        Err(e) => {
            return ToolResult {
                success: false,
                output: format!("Compilation error: {}", e),
                script: None,
            };
        }
    };

    match frame.invoke(&plan) {
        Ok(exec) => {
            let raw_output = if exec.success {
                if exec.stdout.is_empty() {
                    "(no output)".to_string()
                } else {
                    exec.stdout
                }
            } else {
                let code = exec.exit_code.unwrap_or(1);
                if exec.stderr.is_empty() && exec.stdout.is_empty() {
                    format!("Command failed with exit code {}", code)
                } else {
                    format!(
                        "Exit code {}:\n{}\n{}",
                        code,
                        exec.stderr.trim(),
                        exec.stdout.trim()
                    )
                }
            };
            ToolResult {
                success: exec.success,
                output: truncate(&raw_output, MAX_OUTPUT_CHARS),
                script,
            }
        }
        Err(e) => {
            let msg = match &e {
                InvokeError::ExecError(s) if s.contains("run racket") => {
                    "Racket is not installed. Install with: brew install racket".to_string()
                }
                _ => format!("Execution error: {}", e),
            };
            ToolResult {
                success: false,
                output: msg,
                script,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Execute a pre-built PlanDef (from NL pipeline or plan files)
// ---------------------------------------------------------------------------

/// Execute a compiled PlanDef directly and return the result.
///
/// Used when the NL pipeline has already built a plan (e.g., from Earley
/// parsing or plan file lookup). Bypasses op name lookup since we already
/// have the full plan.
pub fn execute_plan_def(plan: &PlanDef) -> ToolResult {
    let frame = DefaultFrame::from_plan(plan);

    let script = match frame.codegen(plan) {
        Ok(s) => Some(s),
        Err(e) => {
            return ToolResult {
                success: false,
                output: format!("Compilation error: {}", e),
                script: None,
            };
        }
    };

    match frame.invoke(plan) {
        Ok(exec) => {
            let raw_output = if exec.success {
                if exec.stdout.is_empty() {
                    "(no output)".to_string()
                } else {
                    exec.stdout
                }
            } else {
                let code = exec.exit_code.unwrap_or(1);
                if exec.stderr.is_empty() && exec.stdout.is_empty() {
                    format!("Command failed with exit code {}", code)
                } else {
                    format!(
                        "Exit code {}:\n{}\n{}",
                        code,
                        exec.stderr.trim(),
                        exec.stdout.trim()
                    )
                }
            };
            ToolResult {
                success: exec.success,
                output: truncate(&raw_output, MAX_OUTPUT_CHARS),
                script,
            }
        }
        Err(e) => {
            let msg = match &e {
                InvokeError::ExecError(s) if s.contains("run racket") => {
                    "Racket is not installed. Install with: brew install racket".to_string()
                }
                _ => format!("Execution error: {}", e),
            };
            ToolResult {
                success: false,
                output: msg,
                script,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Synthetic tool execution (bypass Cadmus pipeline)
// ---------------------------------------------------------------------------

/// Execute a synthetic tool directly (not through the Cadmus plan pipeline).
fn execute_synthetic(op_name: &str, args: &HashMap<String, String>) -> ToolResult {
    match op_name {
        "write_file" => {
            let path = match args.get("path") {
                Some(p) => p,
                None => return ToolResult {
                    success: false,
                    output: "Missing required parameter 'path'".into(),
                    script: None,
                },
            };
            let content = match args.get("content") {
                Some(c) => c,
                None => return ToolResult {
                    success: false,
                    output: "Missing required parameter 'content'".into(),
                    script: None,
                },
            };

            // Expand tilde
            let expanded = expand_tilde(path);

            // Create parent directories if needed
            if let Some(parent) = std::path::Path::new(&expanded).parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            match std::fs::write(&expanded, content) {
                Ok(()) => ToolResult {
                    success: true,
                    output: format!("Wrote {} bytes to {}", content.len(), path),
                    script: None,
                },
                Err(e) => ToolResult {
                    success: false,
                    output: format!("Failed to write {}: {}", path, e),
                    script: None,
                },
            }
        }
        "read_file" => {
            let path = match args.get("path") {
                Some(p) => p,
                None => return ToolResult {
                    success: false,
                    output: "Missing required parameter 'path'".into(),
                    script: None,
                },
            };

            let expanded = expand_tilde(path);

            match std::fs::read_to_string(&expanded) {
                Ok(content) => ToolResult {
                    success: true,
                    output: truncate(&content, MAX_OUTPUT_CHARS),
                    script: None,
                },
                Err(e) => ToolResult {
                    success: false,
                    output: format!("Failed to read {}: {}", path, e),
                    script: None,
                },
            }
        }
        "shell" => {
            let command = match args.get("command") {
                Some(c) => c,
                None => return ToolResult {
                    success: false,
                    output: "Missing required parameter 'command'".into(),
                    script: None,
                },
            };

            match std::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .output()
            {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let combined = if stderr.is_empty() {
                        stdout.to_string()
                    } else if stdout.is_empty() {
                        stderr.to_string()
                    } else {
                        format!("{}\n{}", stdout, stderr)
                    };
                    ToolResult {
                        success: output.status.success(),
                        output: truncate(
                            if combined.is_empty() { "(no output)" } else { &combined },
                            MAX_OUTPUT_CHARS,
                        ),
                        script: None,
                    }
                }
                Err(e) => ToolResult {
                    success: false,
                    output: format!("Failed to run command: {}", e),
                    script: None,
                },
            }
        }
        _ => ToolResult {
            success: false,
            output: format!("Unknown synthetic tool: '{}'", op_name),
            script: None,
        },
    }
}

/// Expand ~ to home directory.
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = std::env::var("HOME").ok() {
            return format!("{}{}", home, &path[1..]);
        }
    }
    path.to_string()
}

/// Truncate a string to at most `max_chars` characters, appending a notice
/// if truncation occurred.
fn truncate(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        s.to_string()
    } else {
        let truncated = &s[..max_chars];
        // Find last newline to avoid cutting mid-line
        let cut = truncated.rfind('\n').unwrap_or(max_chars);
        format!(
            "{}\n\n... [output truncated, {} total chars]",
            &s[..cut],
            s.len()
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unknown_op_returns_error() {
        let args = HashMap::new();
        let result = execute_tool("nonexistent_op_xyz", &args, false);
        assert!(!result.success);
        assert!(result.output.contains("Unknown operation"));
    }

    #[test]
    fn test_write_op_rejected_in_read_only() {
        let mut args = HashMap::new();
        args.insert("file".into(), "test.rs".into());
        args.insert("find".into(), "old".into());
        args.insert("replace".into(), "new".into());
        let result = execute_tool("sed_replace", &args, true);
        assert!(!result.success);
        assert!(result.output.contains("write op"));
        assert!(result.output.contains("read-only"));
    }

    #[test]
    fn test_write_op_allowed_when_not_read_only() {
        // sed_replace will still go through compilation — it won't be rejected
        // at the tool_executor level, though execution may fail if file doesn't exist
        let mut args = HashMap::new();
        args.insert("file".into(), "/tmp/cadmus_test_nonexistent_file.txt".into());
        args.insert("find".into(), "old".into());
        args.insert("replace".into(), "new".into());
        let result = execute_tool("sed_replace", &args, false);
        // The op is allowed (not rejected), even if execution fails
        assert!(result.script.is_some(), "should have compiled a script");
    }

    #[test]
    fn test_grep_code_compiles_and_has_script() {
        let mut args = HashMap::new();
        args.insert("dir".into(), ".".into());
        args.insert("pattern".into(), "test_pattern_unlikely_to_exist_xyz".into());
        let result = execute_tool("grep_code", &args, false);
        // Even if grep finds nothing, the script should have been generated
        assert!(result.script.is_some(), "should generate a script, got: {}", result.output);
        let script = result.script.as_ref().unwrap();
        assert!(script.contains("#lang racket"), "script should be racket");
        assert!(
            script.contains("grep-code"),
            "script should contain grep-code: {}",
            script
        );
    }

    #[test]
    fn test_grep_code_executes() {
        // Search for something that definitely exists in this codebase
        let mut args = HashMap::new();
        args.insert("dir".into(), ".".into());
        args.insert("pattern".into(), "fn main".into());
        let result = execute_tool("grep_code", &args, false);
        // This should succeed if racket is installed
        if result.success {
            assert!(
                result.output.contains("main"),
                "output should contain 'main': {}",
                result.output
            );
        }
        // If racket is not installed, we get an exec error — that's fine
    }

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 100), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        let long = "a\n".repeat(5000);
        let result = truncate(&long, 100);
        assert!(result.len() < 200); // truncated + notice
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_find_definition_compiles() {
        let mut args = HashMap::new();
        args.insert("dir".into(), ".".into());
        args.insert("name".into(), "main".into());
        let result = execute_tool("find_definition", &args, false);
        assert!(result.script.is_some(), "should compile");
    }

    #[test]
    fn test_read_only_allows_read_ops() {
        let mut args = HashMap::new();
        args.insert("dir".into(), ".".into());
        args.insert("pattern".into(), "TODO".into());
        let result = execute_tool("grep_code", &args, true);
        // Should not be rejected — grep_code is a read op
        assert!(
            !result.output.contains("read-only"),
            "read op should not be rejected in read-only mode"
        );
    }

    // -- Synthetic tool tests --

    #[test]
    fn test_write_file_synthetic() {
        let tmp = std::env::temp_dir().join("cadmus_test_write_synthetic.txt");
        let path_str = tmp.to_string_lossy().to_string();
        let mut args = HashMap::new();
        args.insert("path".into(), path_str.clone());
        args.insert("content".into(), "hello from cadmus".into());
        let result = execute_tool("write_file", &args, false);
        assert!(result.success, "write_file should succeed: {}", result.output);
        assert!(result.output.contains("Wrote"));
        // Verify file was actually written
        let content = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(content, "hello from cadmus");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_write_file_blocked_read_only() {
        let mut args = HashMap::new();
        args.insert("path".into(), "/tmp/test.txt".into());
        args.insert("content".into(), "test".into());
        let result = execute_tool("write_file", &args, true);
        assert!(!result.success);
        assert!(result.output.contains("write op") || result.output.contains("read-only"));
    }

    #[test]
    fn test_read_file_synthetic() {
        let tmp = std::env::temp_dir().join("cadmus_test_read_synthetic.txt");
        std::fs::write(&tmp, "test content here").unwrap();
        let mut args = HashMap::new();
        args.insert("path".into(), tmp.to_string_lossy().to_string());
        let result = execute_tool("read_file", &args, false);
        assert!(result.success, "read_file should succeed: {}", result.output);
        assert!(result.output.contains("test content here"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_read_file_allowed_read_only() {
        let tmp = std::env::temp_dir().join("cadmus_test_read_ro.txt");
        std::fs::write(&tmp, "readonly test").unwrap();
        let mut args = HashMap::new();
        args.insert("path".into(), tmp.to_string_lossy().to_string());
        let result = execute_tool("read_file", &args, true);
        assert!(result.success, "read_file should work in read-only mode");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_shell_synthetic() {
        let mut args = HashMap::new();
        args.insert("command".into(), "echo hello_cadmus".into());
        let result = execute_tool("shell", &args, false);
        assert!(result.success, "shell should succeed: {}", result.output);
        assert!(result.output.contains("hello_cadmus"));
    }

    #[test]
    fn test_shell_blocked_read_only() {
        let mut args = HashMap::new();
        args.insert("command".into(), "echo test".into());
        let result = execute_tool("shell", &args, true);
        assert!(!result.success);
        assert!(result.output.contains("write op") || result.output.contains("read-only"));
    }

    #[test]
    fn test_write_file_missing_params() {
        let args = HashMap::new();
        let result = execute_tool("write_file", &args, false);
        assert!(!result.success);
        assert!(result.output.contains("Missing"));
    }

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/test.txt");
        assert!(!expanded.starts_with("~"), "tilde should be expanded: {}", expanded);
        assert!(expanded.contains("test.txt"));

        let no_tilde = expand_tilde("/tmp/test.txt");
        assert_eq!(no_tilde, "/tmp/test.txt");
    }
}
