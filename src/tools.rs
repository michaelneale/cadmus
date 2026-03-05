// ---------------------------------------------------------------------------
// tools — OpenAI-compatible tool definitions from the Cadmus registry
// ---------------------------------------------------------------------------
//
// Generates JSON tool schemas suitable for OpenAI `/v1/chat/completions`
// `tools` parameter. Each tool maps 1:1 to a Cadmus operation. The LLM
// picks tools and provides arguments; Cadmus validates, compiles, and
// executes through its full typed pipeline.
//
// Zero network calls. All definitions are built from the in-memory
// OperationRegistry loaded from YAML ops packs.

use serde_json::Value;

// ---------------------------------------------------------------------------
// Curated op set for agent use
// ---------------------------------------------------------------------------

/// Operations exposed to the agent LLM.
///
/// This is a curated subset — not every registry op makes sense for an
/// agent. We include:
///   - Code search/navigation (grep, find_definition, find_usages, etc.)
///   - Code editing (sed_replace, fix_import, add_after, remove_lines)
///   - Build/test/lint
///   - File inspection (file_outline, list_source_files, recently_changed)
///   - Filesystem ops (walk_tree, list_dir, read_file, find_matching)
///
/// Write ops (sed_replace, fix_import, add_after, remove_lines, fix_assertion,
/// delete, rename, move_entry) are tagged separately for safety filtering.
const AGENT_OPS: &[&str] = &[
    // code search
    "grep_code",
    "find_definition",
    "find_usages",
    "find_imports",
    "file_outline",
    "list_source_files",
    "recently_changed",
    // code editing (write ops)
    "sed_replace",
    "fix_import",
    "add_after",
    "remove_lines",
    "fix_assertion",
    // build/test/lint
    "build_project",
    "test_project",
    "lint_project",
    // macOS tasks
    "open_file",
    // filesystem (these have named parameters via code_editing/power_tools packs)
    // NOTE: polymorphic fs.ops like walk_tree, list_dir, read_file are excluded
    // because they use typed inputs (Dir(a), File(a)) without named parameters.
    // The code_editing ops cover the same functionality with string-named params.
];

/// Registry operations that modify files or state. Used to enforce read-only mode.
/// (Synthetic write ops are handled via `SYNTHETIC_WRITE_OPS`.)
const WRITE_OPS: &[&str] = &[
    "sed_replace",
    "fix_import",
    "add_after",
    "remove_lines",
    "fix_assertion",
    "delete",
    "rename",
    "move_entry",
    "copy",
    "open_file",
];

/// Check if an op is a write operation.
pub fn is_write_op(name: &str) -> bool {
    WRITE_OPS.contains(&name) || SYNTHETIC_WRITE_OPS.iter().any(|s| s.name == name)
}

// ---------------------------------------------------------------------------
// Synthetic tools — not in the Cadmus registry, handled directly
// ---------------------------------------------------------------------------

/// A tool that's handled directly by the tool_executor, not through the
/// Cadmus plan pipeline. Used for operations that don't fit the typed
/// op model (e.g., writing arbitrary content to files, running shell commands).
pub struct SyntheticTool {
    pub name: &'static str,
    pub description: &'static str,
    pub params: &'static [(&'static str, &'static str)], // (name, description)
    pub is_write: bool,
}

/// Synthetic write tools — these bypass the Cadmus pipeline.
const SYNTHETIC_WRITE_OPS: &[SyntheticTool] = &[
    SyntheticTool {
        name: "write_file",
        description: "Write text content to a file. Creates the file if it doesn't exist, overwrites if it does. Use for generating HTML, config files, scripts, etc.",
        params: &[
            ("path", "File path to write to"),
            ("content", "The full text content to write"),
        ],
        is_write: true,
    },
    SyntheticTool {
        name: "shell",
        description: "Run a shell command and return its output. Use for commands not covered by other tools (e.g., 'open index.html', 'python3 script.py', 'npm install').",
        params: &[
            ("command", "Shell command to execute"),
        ],
        is_write: true,
    },
];

/// Synthetic read tools.
const SYNTHETIC_READ_OPS: &[SyntheticTool] = &[
    SyntheticTool {
        name: "read_file",
        description: "Read the contents of a text file and return it.",
        params: &[
            ("path", "File path to read"),
        ],
        is_write: false,
    },
];

/// Check if a tool name is a synthetic tool (handled directly, not via Cadmus pipeline).
pub fn is_synthetic(name: &str) -> bool {
    SYNTHETIC_WRITE_OPS.iter().any(|s| s.name == name)
        || SYNTHETIC_READ_OPS.iter().any(|s| s.name == name)
}

// ---------------------------------------------------------------------------
// Tool definition generation
// ---------------------------------------------------------------------------

/// Generate OpenAI function-calling tool definitions from the Cadmus registry.
///
/// Returns a Vec of JSON objects in OpenAI tool format:
/// ```json
/// {
///   "type": "function",
///   "function": {
///     "name": "grep_code",
///     "description": "recursively search for a pattern in source files",
///     "parameters": {
///       "type": "object",
///       "properties": { "dir": { "type": "string" }, "pattern": { "type": "string" } },
///       "required": ["dir", "pattern"]
///     }
///   }
/// }
/// ```
///
/// If `read_only` is true, write ops are excluded from the tool list.
pub fn tool_definitions(read_only: bool) -> Vec<Value> {
    let reg = crate::fs_types::build_full_registry();
    let mut tools = Vec::new();

    for &name in AGENT_OPS {
        if read_only && is_write_op(name) {
            continue;
        }

        if let Some(entry) = reg.get_poly(name) {
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();

            for (pname, ptype) in entry.input_names.iter().zip(entry.signature.inputs.iter()) {
                properties.insert(
                    pname.clone(),
                    serde_json::json!({
                        "type": "string",
                        "description": format!("{}", ptype)
                    }),
                );
                required.push(serde_json::Value::String(pname.clone()));
            }

            // Clean up description — strip the "(racket-symbol args) — " prefix
            let desc = clean_description(&entry.description);

            tools.push(serde_json::json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": desc,
                    "parameters": {
                        "type": "object",
                        "properties": properties,
                        "required": required
                    }
                }
            }));
        }
    }

    // Add synthetic tools (not in registry, handled directly)
    let all_synthetics = SYNTHETIC_READ_OPS.iter().chain(SYNTHETIC_WRITE_OPS.iter());
    for synth in all_synthetics {
        if read_only && synth.is_write {
            continue;
        }
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        for (pname, pdesc) in synth.params {
            properties.insert(
                pname.to_string(),
                serde_json::json!({
                    "type": "string",
                    "description": pdesc
                }),
            );
            required.push(serde_json::Value::String(pname.to_string()));
        }
        tools.push(serde_json::json!({
            "type": "function",
            "function": {
                "name": synth.name,
                "description": synth.description,
                "parameters": {
                    "type": "object",
                    "properties": properties,
                    "required": required
                }
            }
        }));
    }

    tools
}

/// Generate a compact text catalog of available tools for system prompts.
///
/// Format: `op_name(param1, param2): description`
/// One line per op. Used when the LLM backend doesn't support structured
/// tool definitions (e.g., raw prompt-based tool calling).
pub fn tool_catalog(read_only: bool) -> String {
    let reg = crate::fs_types::build_full_registry();
    let mut catalog = String::new();

    for &name in AGENT_OPS {
        if read_only && is_write_op(name) {
            continue;
        }

        if let Some(entry) = reg.get_poly(name) {
            let params = if entry.input_names.is_empty() {
                entry
                    .signature
                    .inputs
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                entry.input_names.join(", ")
            };
            let desc = clean_description(&entry.description);
            catalog.push_str(&format!("- {name}({params}): {desc}\n"));
        }
    }

    // Add synthetic tools
    let all_synthetics = SYNTHETIC_READ_OPS.iter().chain(SYNTHETIC_WRITE_OPS.iter());
    for synth in all_synthetics {
        if read_only && synth.is_write {
            continue;
        }
        let params: Vec<&str> = synth.params.iter().map(|(n, _)| *n).collect();
        catalog.push_str(&format!("- {}({}): {}\n", synth.name, params.join(", "), synth.description));
    }

    catalog
}

// ---------------------------------------------------------------------------
// Phase 3: Context-aware tool selection
// ---------------------------------------------------------------------------

/// Domain keyword → ops pack mapping. Each domain has trigger keywords
/// and the ops to inject when those keywords appear in the task.
struct DomainHint {
    keywords: &'static [&'static str],
    pack: &'static str,
}

const DOMAIN_HINTS: &[DomainHint] = &[
    DomainHint {
        keywords: &["csv", "text", "string", "split", "join", "word", "count",
                     "uppercase", "lowercase", "trim", "pad", "regex", "parse",
                     "substring", "char", "reverse"],
        pack: "text_processing",
    },
    DomainHint {
        keywords: &["mean", "average", "median", "mode", "variance", "deviation",
                     "percentile", "correlation", "standard", "stats", "statistical",
                     "histogram", "z-score", "zscore", "quartile"],
        pack: "statistics",
    },
    DomainHint {
        keywords: &["trash", "recent", "modified", "spotlight", "launchctl",
                     "desktop", "cleanup", "organize", "renamed", "dated"],
        pack: "macos_tasks",
    },
    DomainHint {
        keywords: &["http", "server", "route", "web", "port", "html", "endpoint"],
        pack: "web",
    },
    DomainHint {
        keywords: &["git", "commit", "push", "pull", "merge", "rebase", "branch",
                     "clone", "stash", "cherry", "tag", "remote", "fetch", "log",
                     "blame", "bisect"],
        pack: "power_tools",
    },
];

/// Generate a context-aware tool catalog. Starts with the base 19 tools,
/// then adds domain-specific ops based on keywords found in the task.
/// Also includes matching plan templates as composite tools.
pub fn contextual_catalog(task: &str, read_only: bool) -> String {
    let mut catalog = tool_catalog(read_only);

    let task_lower = task.to_lowercase();
    let task_words: Vec<&str> = task_lower.split_whitespace().collect();

    // Collect extra ops from matched domains
    let mut added_ops = std::collections::HashSet::new();

    // Mark base ops as already present
    for &name in AGENT_OPS {
        added_ops.insert(name.to_string());
    }

    let mut extra_section = String::new();

    for hint in DOMAIN_HINTS {
        let matched = hint.keywords.iter().any(|kw| task_words.iter().any(|w| w.contains(kw)));
        if !matched {
            continue;
        }

        // Load all ops from this pack that have input_names
        let pack_path = format!("data/packs/ops/{}.ops.yaml", hint.pack);
        if let Ok(content) = std::fs::read_to_string(&pack_path) {
            if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                if let Some(ops) = yaml["ops"].as_sequence() {
                    for op in ops {
                        let name = match op["name"].as_str() {
                            Some(n) => n,
                            None => continue,
                        };
                        if added_ops.contains(name) {
                            continue;
                        }
                        let input_names: Vec<&str> = op["input_names"]
                            .as_sequence()
                            .map(|seq| seq.iter().filter_map(|v| v.as_str()).collect())
                            .unwrap_or_default();
                        if input_names.is_empty() {
                            continue; // can't call without named params
                        }
                        if read_only && is_write_op(name) {
                            continue;
                        }
                        let desc = op["description"]
                            .as_str()
                            .map(|d| clean_description(d))
                            .unwrap_or_default();
                        extra_section.push_str(&format!(
                            "- {}({}): {}\n",
                            name,
                            input_names.join(", "),
                            desc,
                        ));
                        added_ops.insert(name.to_string());
                    }
                }
            }
        }
    }

    // Add matching plans as composite tools
    let plan_section = contextual_plans(task, read_only);

    if !extra_section.is_empty() {
        catalog.push_str("\nDomain-specific tools:\n");
        catalog.push_str(&extra_section);
    }

    if !plan_section.is_empty() {
        catalog.push_str("\nComposite plans (multi-step, call as single tool):\n");
        catalog.push_str(&plan_section);
    }

    catalog
}

/// Find plans whose names or descriptions match task keywords.
/// Returns them formatted as callable tools with parameter descriptions.
fn contextual_plans(task: &str, _read_only: bool) -> String {
    let task_lower = task.to_lowercase();
    const STOP_WORDS: &[&str] = &[
        "the", "and", "for", "but", "not", "you", "all", "can", "her",
        "was", "one", "our", "out", "are", "has", "his", "how", "its",
        "may", "new", "now", "old", "see", "way", "who", "did", "get",
        "let", "say", "she", "too", "use", "with", "from", "this", "that",
        "then", "them", "they", "into", "show", "file", "files", "please",
    ];
    let task_words: Vec<&str> = task_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOP_WORDS.contains(w))
        .collect();

    let mut plan_lines = String::new();
    let mut count = 0;
    const MAX_PLANS: usize = 10;

    let plan_dir = std::path::Path::new("data/plans");
    if !plan_dir.exists() {
        return plan_lines;
    }

    // Scan top-level plans (not algorithm subdirs — those are too many)
    if let Ok(entries) = std::fs::read_dir(plan_dir) {
        for entry in entries.flatten() {
            if count >= MAX_PLANS {
                break;
            }
            let path = entry.path();
            if path.extension().map(|e| e != "sexp").unwrap_or(true) {
                continue;
            }
            let stem = path.file_stem().unwrap().to_string_lossy();
            let stem_words: Vec<&str> = stem.split('_').collect();

            // Check if any task word appears in the plan name
            let name_match = task_words.iter().any(|tw| {
                stem_words.iter().any(|sw| sw.contains(tw) || tw.contains(sw))
            });
            if !name_match {
                continue;
            }

            // Read the plan to get description and params
            if let Ok(content) = std::fs::read_to_string(&path) {
                let desc = content.lines().next()
                    .and_then(|l| l.strip_prefix(";;"))
                    .map(|l| l.trim())
                    .unwrap_or("");

                // Extract params from define line
                let params = content.lines()
                    .find(|l| l.contains("(define"))
                    .and_then(|l| {
                        // (define (name (p1 : T1) (p2 : T2)) ...)
                        let inner = l.trim();
                        let names: Vec<&str> = inner
                            .match_indices("(")
                            .skip(2) // skip "(define" and "(name"
                            .filter_map(|(i, _)| {
                                let rest = &inner[i+1..];
                                rest.split_whitespace().next()
                            })
                            .filter(|w| !w.starts_with(':') && *w != "bind" && *w != "define")
                            .collect();
                        if names.is_empty() { None } else { Some(names.join(", ")) }
                    })
                    .unwrap_or_default();

                plan_lines.push_str(&format!(
                    "- plan:{}({}): {}\n",
                    stem, params, desc,
                ));
                count += 1;
            }
        }
    }

    plan_lines
}

/// Return the list of available agent op names (respecting read_only).
pub fn available_ops(read_only: bool) -> Vec<&'static str> {
    if read_only {
        AGENT_OPS
            .iter()
            .copied()
            .filter(|n| !is_write_op(n))
            .collect()
    } else {
        AGENT_OPS.to_vec()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip the "(racket-symbol args) — " prefix from op descriptions.
fn clean_description(desc: &str) -> String {
    if let Some(idx) = desc.find(" — ") {
        desc[idx + " — ".len()..].to_string()
    } else if let Some(idx) = desc.find(" - ") {
        desc[idx + " - ".len()..].to_string()
    } else {
        desc.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions_non_empty() {
        let tools = tool_definitions(false);
        assert!(!tools.is_empty(), "should generate at least one tool");
    }

    #[test]
    fn test_tool_definitions_have_required_fields() {
        let tools = tool_definitions(false);
        for tool in &tools {
            assert_eq!(tool["type"], "function");
            assert!(tool["function"]["name"].is_string());
            assert!(tool["function"]["description"].is_string());
            assert!(tool["function"]["parameters"]["type"] == "object");
            assert!(tool["function"]["parameters"]["properties"].is_object());
            assert!(tool["function"]["parameters"]["required"].is_array());
        }
    }

    #[test]
    fn test_tool_definitions_include_grep_code() {
        let tools = tool_definitions(false);
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t["function"]["name"].as_str())
            .collect();
        assert!(names.contains(&"grep_code"), "should include grep_code: {:?}", names);
    }

    #[test]
    fn test_grep_code_has_dir_and_pattern() {
        let tools = tool_definitions(false);
        let grep = tools
            .iter()
            .find(|t| t["function"]["name"] == "grep_code")
            .expect("grep_code should exist");
        let props = &grep["function"]["parameters"]["properties"];
        assert!(props["dir"].is_object(), "should have dir param");
        assert!(props["pattern"].is_object(), "should have pattern param");
    }

    #[test]
    fn test_read_only_excludes_write_ops() {
        let rw_tools = tool_definitions(false);
        let ro_tools = tool_definitions(true);
        assert!(ro_tools.len() < rw_tools.len(), "read-only should have fewer tools");

        let ro_names: Vec<&str> = ro_tools
            .iter()
            .filter_map(|t| t["function"]["name"].as_str())
            .collect();
        assert!(
            !ro_names.contains(&"sed_replace"),
            "read-only should not include sed_replace"
        );
        assert!(
            ro_names.contains(&"grep_code"),
            "read-only should include grep_code"
        );
    }

    #[test]
    fn test_tool_catalog_non_empty() {
        let catalog = tool_catalog(false);
        assert!(!catalog.is_empty());
        assert!(catalog.contains("grep_code"));
    }

    #[test]
    fn test_tool_catalog_read_only() {
        let catalog = tool_catalog(true);
        assert!(!catalog.contains("sed_replace"));
        assert!(catalog.contains("grep_code"));
    }

    #[test]
    fn test_is_write_op() {
        assert!(is_write_op("sed_replace"));
        assert!(is_write_op("remove_lines"));
        assert!(!is_write_op("grep_code"));
        assert!(!is_write_op("find_definition"));
    }

    #[test]
    fn test_available_ops_read_only() {
        let all = available_ops(false);
        let ro = available_ops(true);
        assert!(ro.len() < all.len());
        assert!(!ro.contains(&"sed_replace"));
        assert!(ro.contains(&"grep_code"));
    }

    #[test]
    fn test_clean_description() {
        assert_eq!(
            clean_description("(grep-code dir pattern) — recursively search for a pattern"),
            "recursively search for a pattern"
        );
        assert_eq!(
            clean_description("no prefix here"),
            "no prefix here"
        );
    }

    #[test]
    fn test_tool_descriptions_are_clean() {
        let tools = tool_definitions(false);
        for tool in &tools {
            let desc = tool["function"]["description"].as_str().unwrap();
            assert!(
                !desc.starts_with('('),
                "description should not start with '(' (racket prefix): {}",
                desc
            );
        }
    }

    // -- Phase 3: contextual selection tests --

    #[test]
    fn test_contextual_catalog_includes_git_ops_for_git_task() {
        let catalog = contextual_catalog("commit and push my changes", false);
        assert!(catalog.contains("git"), "git task should inject git ops: {}", catalog);
    }

    #[test]
    fn test_contextual_catalog_includes_stats_for_stats_task() {
        let catalog = contextual_catalog("compute the mean and variance of this data", false);
        assert!(catalog.contains("mean_list") || catalog.contains("variance"),
            "stats task should inject stats ops: {}", catalog);
    }

    #[test]
    fn test_contextual_catalog_includes_text_ops_for_csv_task() {
        let catalog = contextual_catalog("parse this CSV file and count words", false);
        assert!(catalog.contains("csv") || catalog.contains("word_count"),
            "csv task should inject text ops: {}", catalog);
    }

    #[test]
    fn test_contextual_catalog_no_extras_for_generic_task() {
        let catalog = contextual_catalog("find bugs in the code", false);
        // Should not have domain-specific sections
        assert!(!catalog.contains("Domain-specific"),
            "generic task should not inject domain ops: {}", catalog);
    }

    #[test]
    fn test_contextual_catalog_includes_plans_for_matching_task() {
        let catalog = contextual_catalog("commit and push to remote", false);
        assert!(catalog.contains("plan:") || catalog.contains("commit"),
            "matching task should surface relevant plans: {}", catalog);
    }

    #[test]
    fn test_contextual_catalog_always_has_base_tools() {
        let catalog = contextual_catalog("compute statistics on data", false);
        assert!(catalog.contains("grep_code"), "should always include base tools");
        assert!(catalog.contains("write_file"), "should always include synthetic tools");
    }
}
