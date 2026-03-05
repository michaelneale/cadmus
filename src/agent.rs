// ---------------------------------------------------------------------------
// agent — LLM agent loop using Cadmus ops via text-based ACTION protocol
// ---------------------------------------------------------------------------
//
// Drives a local LLM through a task using a simple text protocol:
//   - LLM outputs:  ACTION: tool_name(param=value, param=value)
//   - Cadmus executes the tool and returns: RESULT: <output>
//   - LLM sees the result and picks the next action (or gives final answer)
//
// This is deliberately NOT OpenAI tool-calling. Small local models (3B-4B)
// are much better at outputting structured text than at tool_calls JSON.
// The ACTION: format is trivially parseable and LLM-friendly.
//
// Architecture:
//   - System prompt lists available ops in plain text (from the registry)
//   - Each action goes through tool_executor (Cadmus typed pipeline or synthetic)
//   - LLM never sees Racket code, sexpr plans, or shell commands
//   - Communication via OpenAI-compatible /v1/chat/completions (text only, no tools param)
//
// Works with: Ollama, llama.cpp server, LM Studio, vLLM, or any OpenAI API.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools;
use crate::tool_executor;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const DEFAULT_LLM_URL: &str = "http://localhost:11434/v1/chat/completions";
const DEFAULT_MODEL: &str = "glm-4.7-flash:latest";
const DEFAULT_MAX_STEPS: usize = 15;
const DEFAULT_MAX_TOKENS: usize = 4000;
const MAX_OUTPUT_DISPLAY: usize = 200;

/// Agent configuration.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// OpenAI-compatible API endpoint.
    pub llm_url: String,
    /// Model name to use.
    pub model: String,
    /// Maximum number of action steps before stopping.
    pub max_steps: usize,
    /// If true, write ops (sed_replace, write_file, shell, etc.) are disabled.
    pub read_only: bool,
    /// Sampling temperature.
    pub temperature: f64,
    /// Max tokens per LLM response.
    pub max_tokens: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            llm_url: std::env::var("CADMUS_LLM_URL")
                .unwrap_or_else(|_| DEFAULT_LLM_URL.to_string()),
            model: std::env::var("CADMUS_MODEL")
                .unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
            max_steps: DEFAULT_MAX_STEPS,
            read_only: false,
            temperature: 0.1,
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }
}

// ---------------------------------------------------------------------------
// Messages (simple — no tool_calls, just role + content)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn system(content: &str) -> Self {
        Self { role: "system".into(), content: content.into() }
    }
    pub fn user(content: &str) -> Self {
        Self { role: "user".into(), content: content.into() }
    }
    pub fn assistant(content: &str) -> Self {
        Self { role: "assistant".into(), content: content.into() }
    }
}

// ---------------------------------------------------------------------------
// Agent result
// ---------------------------------------------------------------------------

/// A single step in the agent's execution trace.
#[derive(Debug, Clone)]
pub struct AgentStep {
    pub step: usize,
    pub tool_name: Option<String>,
    pub tool_args: Option<HashMap<String, String>>,
    pub success: bool,
    pub output: String,
}

/// Result of running the agent loop.
#[derive(Debug, Clone)]
pub struct AgentResult {
    pub completed: bool,
    pub summary: String,
    pub steps: Vec<AgentStep>,
    pub tool_calls: usize,
}

// ---------------------------------------------------------------------------
// System prompt
// ---------------------------------------------------------------------------

fn build_system_prompt(task: &str, config: &AgentConfig) -> String {
    let catalog = tools::contextual_catalog(task, config.read_only);
    let mode = if config.read_only {
        "You are in READ-ONLY mode. You cannot modify files or run shell commands."
    } else {
        "You can search, inspect, modify code, write files, and run commands."
    };

    format!(
        r#"You complete tasks step by step using tools.

{mode}

To use a tool, output a line starting with ACTION:
ACTION: tool_name(param="value", param="value")

Available tools:
{catalog}
After each ACTION you will receive the RESULT. Then decide your next ACTION or give your final answer as plain text.

Rules:
1. ONE action per turn. Wait for the result before the next action.
2. Always search/grep before editing. Never edit blind.
3. After editing, verify with build_project or test_project.
4. When done, give a plain text answer (no ACTION line).
5. If a tool fails, try a different approach.
6. Use dir="." for the current project unless told otherwise.
7. For write_file, put the full content in the content parameter."#
    )
}

// ---------------------------------------------------------------------------
// LLM communication
// ---------------------------------------------------------------------------

fn call_llm(config: &AgentConfig, messages: &[Message]) -> Result<String, String> {
    let body = serde_json::json!({
        "model": &config.model,
        "messages": messages,
        "temperature": config.temperature,
        "max_tokens": config.max_tokens,
    });

    let response = ureq::post(&config.llm_url)
        .send_json(&body)
        .map_err(|e| format!("HTTP error: {}", e))?;

    let json: Value = response
        .into_json()
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let choice = json["choices"]
        .get(0)
        .ok_or_else(|| format!("No choices in response: {}", json))?;

    let message = &choice["message"];

    // Handle models that split reasoning/content (like GLM)
    let content = message["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // If content is empty but reasoning has the answer, use reasoning
    if content.is_empty() {
        if let Some(reasoning) = message["reasoning"].as_str() {
            // Check if reasoning contains an ACTION line
            for line in reasoning.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("ACTION:") {
                    return Ok(trimmed.to_string());
                }
            }
            // No action in reasoning — might be the final answer
            if !reasoning.is_empty() {
                return Ok(reasoning.to_string());
            }
        }
    }

    Ok(content)
}

// ---------------------------------------------------------------------------
// Parse ACTION line
// ---------------------------------------------------------------------------

/// Parsed action from LLM output.
#[derive(Debug, Clone)]
struct ParsedAction {
    tool: String,
    args: HashMap<String, String>,
}

/// Parse "ACTION: tool_name(param="value", param="value")" from LLM output.
/// Returns None if the output doesn't contain an ACTION line (= final answer).
fn parse_action(output: &str) -> Option<ParsedAction> {
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("ACTION:") {
            let rest = rest.trim();
            return parse_tool_call(rest);
        }
    }
    None
}

/// Parse "tool_name(param="value", param="value")" or "tool_name(value1, value2)"
fn parse_tool_call(s: &str) -> Option<ParsedAction> {
    let paren = s.find('(')?;
    let tool = s[..paren].trim().to_string();
    if tool.is_empty() {
        return None;
    }

    // Extract content between outermost parens
    let rest = s[paren + 1..].trim();
    let inner = rest.strip_suffix(')')
        .or_else(|| rest.strip_suffix(");"))
        .unwrap_or(rest);

    let args = parse_args(inner);

    Some(ParsedAction { tool, args })
}

/// Parse key="value" pairs or positional args from inside parens.
fn parse_args(s: &str) -> HashMap<String, String> {
    let mut args = HashMap::new();
    if s.trim().is_empty() {
        return args;
    }

    // Try key=value format first
    let mut remaining = s.trim();
    let mut positional_idx = 0;

    while !remaining.is_empty() {
        remaining = remaining.trim_start_matches(|c: char| c == ',' || c.is_whitespace());
        if remaining.is_empty() {
            break;
        }

        // Check for key=value
        if let Some(eq_pos) = remaining.find('=') {
            let before_eq = &remaining[..eq_pos];
            // Make sure there's no quote before the = (it's not a value containing =)
            if !before_eq.contains('"') && !before_eq.contains('\'') && before_eq.chars().all(|c| c.is_alphanumeric() || c == '_') {
                let key = before_eq.trim().to_string();
                remaining = remaining[eq_pos + 1..].trim();

                // Parse the value (may be quoted)
                let (value, rest) = parse_value(remaining);
                args.insert(key, value);
                remaining = rest;
                continue;
            }
        }

        // Positional argument
        let (value, rest) = parse_value(remaining);
        let key = match positional_idx {
            0 => "dir".to_string(),
            1 => "pattern".to_string(),
            _ => format!("arg{}", positional_idx),
        };
        args.insert(key, value);
        positional_idx += 1;
        remaining = rest;
    }

    args
}

/// Parse a single value (quoted or unquoted) from the start of a string.
/// Returns (value, remaining).
fn parse_value(s: &str) -> (String, &str) {
    let s = s.trim();
    if s.starts_with('"') {
        // Quoted string — find matching close quote
        let inner = &s[1..];
        let mut end = 0;
        let mut escaped = false;
        for (i, c) in inner.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                continue;
            }
            if c == '"' {
                end = i;
                break;
            }
            end = i + c.len_utf8();
        }
        let value = inner[..end].replace("\\\"", "\"").replace("\\n", "\n");
        let rest = if end + 1 < inner.len() {
            &inner[end + 1..]
        } else {
            ""
        };
        (value, rest)
    } else {
        // Unquoted — read until comma or closing paren
        let end = s.find(|c: char| c == ',' || c == ')').unwrap_or(s.len());
        let value = s[..end].trim().to_string();
        let rest = if end < s.len() { &s[end..] } else { "" };
        (value, rest)
    }
}

// ---------------------------------------------------------------------------
// NL-first shortcut: try deterministic pipeline before invoking LLM
// ---------------------------------------------------------------------------

/// Attempt to handle the task through the deterministic NL pipeline.
/// Returns `Some(AgentResult)` if the NL pipeline produced a plan and
/// executed it. Returns `None` if it couldn't parse the input (the LLM
/// agent loop should take over).
fn try_nl_shortcut(task: &str, config: &AgentConfig) -> Option<AgentResult> {
    use crate::nl;
    use crate::nl::dialogue::DialogueState;

    let mut state = DialogueState::new();
    let response = nl::process_input(task, &mut state);

    match response {
        nl::NlResponse::PlanCreated { plan_sexpr, summary, .. } => {
            // NL pipeline built a plan — execute it directly
            let plan = match state.current_plan.take() {
                Some(p) => p,
                None => return None,
            };

            // Check read-only: does the plan contain write ops?
            if config.read_only {
                for step in &plan.steps {
                    let op = step.op.as_str();
                    if crate::tools::is_write_op(op) {
                        eprintln!(
                            "  {} NL plan contains write op '{}' but read-only mode is active",
                            crate::ui::dim("✗"),
                            op,
                        );
                        return None; // fall through to LLM which will also be blocked
                    }
                }
            }

            eprintln!();
            eprintln!(
                "  {} NL shortcut — {}",
                crate::ui::status_ok("▸"),
                summary,
            );
            eprintln!();
            eprintln!("  {}", crate::ui::dim(&plan_sexpr.lines().take(5).collect::<Vec<_>>().join("\n  ")));
            eprintln!();

            let result = tool_executor::execute_plan_def(&plan);

            let preview = result.output.lines().take(5).collect::<Vec<_>>().join("\n");
            if result.success {
                eprintln!("  {} {}", crate::ui::dim("→"), crate::ui::dim(&short(&preview, 200)));
                eprintln!();

                Some(AgentResult {
                    completed: true,
                    summary: result.output.clone(),
                    steps: vec![AgentStep {
                        step: 1,
                        tool_name: Some(format!("nl_plan: {}", summary)),
                        tool_args: None,
                        success: true,
                        output: result.output,
                    }],
                    tool_calls: 1,
                })
            } else {
                // NL built a plan but execution failed — fall through to LLM
                eprintln!(
                    "  {} NL plan failed, handing to LLM: {}",
                    crate::ui::dim("▸"),
                    short(&result.output, 80),
                );
                eprintln!();
                None
            }
        }
        _ => None, // NL couldn't parse — fall through to LLM
    }
}

// ---------------------------------------------------------------------------
// Agent loop
// ---------------------------------------------------------------------------

/// Run the agent loop to completion.
pub fn run_agent(task: &str, config: &AgentConfig) -> AgentResult {
    // ── Phase 2: NL-first routing ──────────────────────────────────────
    // Try the deterministic NL pipeline first. If it builds a plan, execute
    // it directly — zero LLM cost, instant response.
    if let Some(result) = try_nl_shortcut(task, config) {
        return result;
    }

    let system_prompt = build_system_prompt(task, config);

    let mut messages = vec![
        Message::system(&system_prompt),
        Message::user(task),
    ];

    let mut steps = Vec::new();
    let mut tool_call_count = 0;
    let mut last_action: Option<String> = None;
    let mut repeat_count = 0;

    eprintln!();
    eprintln!(
        "  {} agent starting — max {} steps",
        crate::ui::status_active("▸"),
        config.max_steps,
    );
    eprintln!();

    for step_num in 1..=config.max_steps {
        let response = match call_llm(config, &messages) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  {} {}", crate::ui::dim("✗ llm error:"), e);
                return AgentResult {
                    completed: false,
                    summary: format!("Agent stopped due to LLM error: {}", e),
                    steps,
                    tool_calls: tool_call_count,
                };
            }
        };

        // Check if the response contains an ACTION
        match parse_action(&response) {
            Some(action) => {
                // Loop detection
                let action_key = format!("{}|{:?}", action.tool, action.args);
                if last_action.as_ref() == Some(&action_key) {
                    repeat_count += 1;
                    if repeat_count >= 2 {
                        eprintln!("    {} repeated action, nudging", crate::ui::dim("⚠"));
                        messages.push(Message::assistant(&response));
                        messages.push(Message::user(
                            "RESULT: You already ran this exact action. The result is the same. \
                             Try a different approach or give your final answer.",
                        ));
                        repeat_count = 0;
                        last_action = None;
                        continue;
                    }
                } else {
                    repeat_count = 0;
                }
                last_action = Some(action_key);

                // Display
                let args_display: Vec<String> = action.args.iter()
                    .map(|(k, v)| format!("{}={}", k, short(v, 40)))
                    .collect();
                eprintln!(
                    "  {} step {}: {}({})",
                    crate::ui::dim("▸"),
                    step_num,
                    action.tool,
                    args_display.join(", "),
                );

                // Execute
                let result = tool_executor::execute_tool(
                    &action.tool,
                    &action.args,
                    config.read_only,
                );
                tool_call_count += 1;

                // Display preview
                let preview = short(&result.output, MAX_OUTPUT_DISPLAY);
                if result.success {
                    eprintln!("    {} {}", crate::ui::dim("→"), crate::ui::dim(&preview));
                } else {
                    eprintln!("    {} {}", crate::ui::dim("✗"), crate::ui::dim(&preview));
                }

                steps.push(AgentStep {
                    step: step_num,
                    tool_name: Some(action.tool.clone()),
                    tool_args: Some(action.args.clone()),
                    success: result.success,
                    output: result.output.clone(),
                });

                // Feed result back to LLM
                messages.push(Message::assistant(&response));
                messages.push(Message::user(&format!("RESULT: {}", result.output)));
            }
            None => {
                // No ACTION — this is the final answer
                let answer = response.trim().to_string();
                if answer.is_empty() {
                    // Empty response — nudge
                    messages.push(Message::assistant(""));
                    messages.push(Message::user(
                        "Continue with the task. Use ACTION: tool_name(...) to run a tool, \
                         or give your final answer as plain text.",
                    ));
                    continue;
                }

                eprintln!();
                eprintln!("  {} {}", crate::ui::dim("agent:"), short(&answer, 200));
                eprintln!();

                steps.push(AgentStep {
                    step: step_num,
                    tool_name: None,
                    tool_args: None,
                    success: true,
                    output: answer.clone(),
                });

                return AgentResult {
                    completed: true,
                    summary: answer,
                    steps,
                    tool_calls: tool_call_count,
                };
            }
        }
    }

    eprintln!(
        "  {} max steps ({}) reached",
        crate::ui::dim("▸"),
        config.max_steps,
    );

    AgentResult {
        completed: false,
        summary: format!(
            "Agent reached maximum steps ({}). Made {} tool calls.",
            config.max_steps, tool_call_count
        ),
        steps,
        tool_calls: tool_call_count,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn short(s: &str, max: usize) -> String {
    let first_line = s.lines().next().unwrap_or("");
    if first_line.len() <= max {
        first_line.to_string()
    } else {
        format!("{}...", &first_line[..max])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_defaults() {
        let config = AgentConfig::default();
        assert!(config.max_steps > 0);
        assert!(!config.read_only);
        assert!(config.temperature < 1.0);
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::system("hello");
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "system");
        assert_eq!(json["content"], "hello");
    }

    #[test]
    fn test_system_prompt_contains_tools() {
        let config = AgentConfig::default();
        let prompt = build_system_prompt("search for bugs", &config);
        assert!(prompt.contains("grep_code"), "should include grep_code");
        assert!(prompt.contains("write_file"), "should include write_file");
        assert!(prompt.contains("ACTION"), "should explain ACTION protocol");
    }

    #[test]
    fn test_system_prompt_read_only() {
        let config = AgentConfig {
            read_only: true,
            ..Default::default()
        };
        let prompt = build_system_prompt("search for bugs", &config);
        assert!(prompt.contains("READ-ONLY"));
        assert!(!prompt.contains("sed_replace"));
    }

    // -- ACTION parsing --

    #[test]
    fn test_parse_action_basic() {
        let action = parse_action(r#"ACTION: grep_code(dir=".", pattern="TODO")"#);
        assert!(action.is_some());
        let a = action.unwrap();
        assert_eq!(a.tool, "grep_code");
        assert_eq!(a.args.get("dir").unwrap(), ".");
        assert_eq!(a.args.get("pattern").unwrap(), "TODO");
    }

    #[test]
    fn test_parse_action_with_preceding_text() {
        let text = "I'll search for compile_plan.\nACTION: find_definition(dir=\".\", name=\"compile_plan\")";
        let action = parse_action(text);
        assert!(action.is_some());
        let a = action.unwrap();
        assert_eq!(a.tool, "find_definition");
        assert_eq!(a.args.get("name").unwrap(), "compile_plan");
    }

    #[test]
    fn test_parse_action_no_action() {
        let action = parse_action("Here is my final answer about the code.");
        assert!(action.is_none());
    }

    #[test]
    fn test_parse_action_positional() {
        let action = parse_action(r#"ACTION: grep_code(".", "fn main")"#);
        assert!(action.is_some());
        let a = action.unwrap();
        assert_eq!(a.tool, "grep_code");
        assert_eq!(a.args.get("dir").unwrap(), ".");
        assert_eq!(a.args.get("pattern").unwrap(), "fn main");
    }

    #[test]
    fn test_parse_action_write_file() {
        let action = parse_action(r#"ACTION: write_file(path="/tmp/test.html", content="<html><body>Hello</body></html>")"#);
        assert!(action.is_some());
        let a = action.unwrap();
        assert_eq!(a.tool, "write_file");
        assert_eq!(a.args.get("path").unwrap(), "/tmp/test.html");
        assert_eq!(a.args.get("content").unwrap(), "<html><body>Hello</body></html>");
    }

    #[test]
    fn test_parse_action_shell() {
        let action = parse_action(r#"ACTION: shell(command="open /tmp/test.html")"#);
        assert!(action.is_some());
        let a = action.unwrap();
        assert_eq!(a.tool, "shell");
        assert_eq!(a.args.get("command").unwrap(), "open /tmp/test.html");
    }

    #[test]
    fn test_parse_action_file_outline() {
        let action = parse_action(r#"ACTION: file_outline(file="src/plan.rs")"#);
        assert!(action.is_some());
        let a = action.unwrap();
        assert_eq!(a.tool, "file_outline");
        assert_eq!(a.args.get("file").unwrap(), "src/plan.rs");
    }

    #[test]
    fn test_parse_action_single_param_no_key() {
        // Some models output: ACTION: file_outline("src/plan.rs")
        let action = parse_action(r#"ACTION: file_outline("src/plan.rs")"#);
        assert!(action.is_some());
        let a = action.unwrap();
        assert_eq!(a.tool, "file_outline");
        // First positional maps to "dir"
        assert!(a.args.values().any(|v| v == "src/plan.rs"));
    }

    #[test]
    fn test_parse_value_quoted() {
        let (val, rest) = parse_value(r#""hello world", next"#);
        assert_eq!(val, "hello world");
        assert!(rest.contains("next"));
    }

    #[test]
    fn test_parse_value_unquoted() {
        let (val, rest) = parse_value("hello, next");
        assert_eq!(val, "hello");
        assert!(rest.contains("next"));
    }

    #[test]
    fn test_parse_value_with_escaped_quotes() {
        let (val, _) = parse_value(r#""say \"hello\"""#);
        assert_eq!(val, r#"say "hello""#);
    }

    #[test]
    fn test_short_truncation() {
        assert_eq!(short("hello", 100), "hello");
        assert_eq!(short("hello world\nsecond line", 100), "hello world");
        let long = "a".repeat(200);
        let result = short(&long, 10);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_agent_result_structure() {
        let result = AgentResult {
            completed: true,
            summary: "Done".into(),
            steps: vec![],
            tool_calls: 0,
        };
        assert!(result.completed);
        assert_eq!(result.tool_calls, 0);
    }
}

    #[test]
    fn test_nl_shortcut_recognizes_known_command() {
        // "find comics in downloads" is a well-tested NL path with bindings
        use crate::nl;
        use crate::nl::dialogue::DialogueState;
        let mut state = DialogueState::new();
        let response = nl::process_input("find comics in downloads", &mut state);
        assert!(matches!(response, nl::NlResponse::PlanCreated { .. }));
        let plan = state.current_plan.as_ref().unwrap();
        assert!(!plan.bindings.is_empty(), "NL should bind the path");
    }

    #[test]
    fn test_nl_shortcut_returns_none_for_unknown() {
        // "write a pacman game" is not parseable by NL — should return None
        let config = AgentConfig::default();
        let result = try_nl_shortcut("write a pacman game in HTML and open it", &config);
        assert!(result.is_none(), "unknown tasks should fall through to LLM");
    }

    #[test]
    fn test_nl_shortcut_recognizes_algorithm() {
        use crate::nl;
        use crate::nl::dialogue::DialogueState;
        let mut state = DialogueState::new();
        let response = nl::process_input("quicksort", &mut state);
        assert!(matches!(response, nl::NlResponse::PlanCreated { .. }));
    }
