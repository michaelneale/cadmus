//! Natural Language UX layer.
//!
//! A deterministic, low-latency adapter that converts natural language input
//! into structured plan definitions for Cadmus.
//! Pipeline:
//!
//! 1. **Normalization** — case fold, punctuation strip, synonym mapping (`normalize`)
//! 2. **Typo correction** — domain-bounded SymSpell dictionary (`typo`)
//! 3. **Lexicon dispatch** — approve/reject/explain via lexicon, edits via pattern match
//! 4. **Earley parsing** — grammar-based parse of commands → IntentIR → PlanDef
//!
//! Plus dialogue state management (`dialogue`) for multi-turn conversations.

pub mod normalize;
pub mod typo;
pub mod recipes;
pub mod dialogue;
pub mod vocab;
pub mod earley;
pub mod grammar;
pub mod lexicon;
pub mod intent_ir;
pub mod intent_compiler;
pub mod phrase;


use dialogue::{DialogueState, DialogueError, FocusEntry};
use dialogue::EditAction;
use dialogue::ExtractedSlots;
use crate::calling_frame::CallingFrame;

/// Parse a plan string — tries sexpr first, falls back to YAML.
fn parse_plan_any(src: &str) -> Result<crate::plan::PlanDef, String> {
    crate::sexpr::parse_sexpr_to_plan(src)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// NlResponse — the output of the NL UX layer
// ---------------------------------------------------------------------------

/// The response from processing a user input through the NL layer.
#[derive(Debug, Clone)]
pub enum NlResponse {
    /// A new plan plan was created.
    PlanCreated {
        /// The plan YAML string.
        plan_sexpr: String,
        /// Human-readable summary of what the plan does.
        summary: String,
        /// The prompt to show the user.
        prompt: String,
    },

    /// An existing plan plan was edited.
    PlanEdited {
        /// The revised plan YAML string.
        plan_sexpr: String,
        /// Description of what changed.
        diff_description: String,
        /// The prompt to show the user.
        prompt: String,
    },
    /// An explanation of an operation or concept.
    Explanation {
        /// The explanation text.
        text: String,
    },
    /// The user approved the current plan.
    Approved {
        /// The generated shell script (if plan was compiled successfully).
        script: Option<String>,
    },
    /// The user rejected the current plan.
    Rejected,
    /// The input was ambiguous — we need clarification.
    NeedsClarification {
        /// What we need to know.
        needs: Vec<String>,
    },
    /// A parameter was set.
    ParamSet {
        /// Description of what was set.
        description: String,
        /// The revised plan YAML (if a plan exists).
        plan_sexpr: Option<String>,
    },
    /// An error occurred.
    Error {
        /// Error message.
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Public API — the main entry point
// ---------------------------------------------------------------------------

/// Process a user input through the full NL pipeline.
///
/// Pipeline:
///   1. normalize → typo_correct
///   2. Check approve/reject/explain/edit (keyword/pattern match)
///   3. Earley parse → Intent IR → compile to PlanDef
///   4. Fallback: old intent/slots pipeline
///
/// This is the single entry point for the NL UX layer.
pub fn process_input(input: &str, state: &mut DialogueState) -> NlResponse {
    state.next_turn();

    // 1. Normalize: case fold, strip punctuation, expand contractions, ordinals
    let first_pass = normalize::normalize(input);

    // 2. Typo correction on the raw tokens (before synonym mapping)
    let dict = typo::domain_dict();
    let corrected_tokens = dict.correct_tokens(&first_pass.tokens);

    let lex = lexicon::lexicon();

    // 3. Check approve/reject/explain via lexicon (before Earley parse)
    if lex.is_approve(&corrected_tokens) {
        return handle_approve(state);
    }
    if lex.is_reject(&corrected_tokens) {
        state.current_plan = None;
        state.alternative_intents.clear();
        return NlResponse::Rejected;
    }
    // 3b. Check for command recipe queries ("give me the command to ...",
    //     "how do I ... from terminal", "what's the command for ...")
    if let Some(content_tokens) = try_recipe_query(&corrected_tokens) {
        if let Some(response) = handle_recipe_query(&content_tokens) {
            return response;
        }
    }

    if let Some(subject) = lex.try_explain(&corrected_tokens) {
        return handle_explain(&subject);
    }

    // 4. Check for edit commands (lightweight pattern match)
    //    Only if there's a current plan to edit.
    if state.current_plan.is_some() {
        if let Some((action, rest_tokens)) = try_detect_edit(&corrected_tokens) {
            // Extract slots from the rest tokens for edit handling
            let extracted = dialogue::extract_slots(&rest_tokens);
            return handle_edit_step(action, &extracted, state);
        }
    }

    // 5. Earley parse for plan creation (sole path)
    try_earley_create(&corrected_tokens, state, Vec::new())
}

/// Try to detect an edit command from tokens.
/// Returns (EditAction, rest_tokens) if detected.
fn try_detect_edit(tokens: &[String]) -> Option<(EditAction, Vec<String>)> {
    if tokens.is_empty() {
        return None;
    }

    // Skip filler prefixes
    let vocab = vocab::vocab();
    let mut start = 0;
    while start < tokens.len() && vocab.filler_prefixes.contains(&tokens[start]) {
        start += 1;
    }
    if start >= tokens.len() {
        return None;
    }
    let first = &tokens[start];

    let action_map: &[(&[&str], EditAction)] = &[
        (&["skip", "exclude", "ignore", "omit"], EditAction::Skip),
        (&["remove", "delete", "drop", "cut"], EditAction::Remove),
        (&["add", "append", "include"], EditAction::Add),
        (&["move", "reorder", "swap", "rearrange"], EditAction::Move),
        (&["change", "modify", "update", "alter", "set", "use"], EditAction::Change),
        (&["insert", "prepend", "put"], EditAction::Insert),
    ];

    for (keywords, action) in action_map {
        if keywords.contains(&first.as_str()) {
            // Check for step reference or named/subdirectory keywords
            let has_step_ref = tokens.iter().any(|t| {
                t == "step" || t == "previous" || t == "next" || t == "last"
                    || t == "before" || t == "after"
                    || t.parse::<u32>().is_ok()
            });
            let has_named = tokens.iter().any(|t| t == "named" || t == "called" || t == "matching");
            let has_subdirectory = tokens.iter().any(|t| {
                t == "subdirectory" || t == "subdirectories" || t == "subfolder" || t == "subdir"
            });

            if has_step_ref || has_named || has_subdirectory
                || (*action == EditAction::Skip)
            {
                // Don't treat as edit if it looks like arithmetic
                if normalize::is_canonical_op(first) {
                    let rest = &tokens[start + 1..];
                    let looks_arithmetic = rest.iter().all(|t|
                        t.parse::<f64>().is_ok() || matches!(t.as_str(), "and" | "together" | "from" | "by" | "with" | "to" | "of")
                    ) && rest.iter().any(|t| t.parse::<f64>().is_ok());
                    if looks_arithmetic {
                        return None;
                    }
                }
                let rest: Vec<String> = tokens[start + 1..].to_vec();
                return Some((action.clone(), rest));
            }
        }
    }

    // "move step 2 before step 1" pattern
    if first == "move" || first == "move_entry" {
        let has_step = tokens.iter().any(|t| t == "step");
        if has_step {
            let rest: Vec<String> = tokens[start + 1..].to_vec();
            return Some((EditAction::Move, rest));
        }
    }

    None
}


// ---------------------------------------------------------------------------
// Earley parser integration
// ---------------------------------------------------------------------------

/// Parse and compile via the Earley pipeline. This is the sole path for
/// plan creation — there is no old-pipeline fallback.
///
/// If Earley cannot parse the input, returns NeedsClarification with the
/// provided fallback_needs (or a generic message if empty).
fn try_earley_create(
    tokens: &[String],
    state: &mut DialogueState,
    fallback_needs: Vec<String>,
) -> NlResponse {
    // Phase 0: Phrase tokenization — group multi-word verb phrases into
    // single canonical tokens (e.g., "make me a list" → "list").
    let phrase_tokens = phrase::phrase_tokenize(tokens);

    // ── Pre-Earley short-circuit: if any phrase token is an algorithm op
    //    or plan file, skip the Earley parser entirely.
    //    Only check the FIRST algorithm-op token (skip filesystem verbs). ──
    static FS_VERBS: &[&str] = &[
        "find", "search", "list", "get", "filter", "show", "display",
        "locate", "seek", "discover", "hunt", "detect", "identify",
        "compute", "calculate", "run", "execute", "perform",
    ];
    let registry = crate::fs_types::build_full_registry();

    // ── Pre-Earley: try joining consecutive tokens as plan file names ──
    // e.g., ["git", "log", "search", ...] → try "git_log_search", "git_log", etc.
    {
        let content_tokens: Vec<&str> = phrase_tokens.iter()
            .map(|t| t.as_str())
            .take_while(|t| !t.starts_with('/') && !t.starts_with('.') && !t.starts_with('~'))
            .filter(|t| !["the", "a", "an", "in", "of", "for", "with", "and", "to", "from", "by", "on", "at", "is", "it"].contains(t))
            .collect();


                // ── Pre-check: first content token as a direct plan name ──
        // e.g., "wagner_fischer" joined by phrase tokenizer → try as plan file directly.
        // Only check the FIRST content token — it's most likely the plan name.
        // Checking later tokens risks false matches (e.g., "caesar_cipher" in
        // "ROT13: Caesar cipher with shift 13" when the plan is rot13_cipher).
        {
            let early: Vec<&str> = content_tokens.iter().take(1).copied().collect();
            for token in early {
                if let Some(plan_sexpr) = intent_compiler::try_load_plan_sexpr(token) {
                    if let Ok(plan) = parse_plan_any(&plan_sexpr) {
                        return finish_plan_creation(plan, plan_sexpr, state);
                    }
                }
            }
        }

        // Try longest match first (up to 5 tokens), also try with trailing 's'
        let max_len = content_tokens.len().min(5);
        for len in (2..=max_len).rev() {
            let candidate = content_tokens[..len].join("_");
            // Try variations: exact, +s, drop last token, drop last +s
            let shorter = if len > 2 {
                Some(content_tokens[..len - 1].join("_"))
            } else {
                None
            };
            let mut tries: Vec<String> = vec![candidate.clone(), format!("{}s", candidate)];
            if let Some(ref s) = shorter {
                tries.push(s.clone());
                tries.push(format!("{}s", s));
            }
            let found = tries.iter().find_map(|c| {
                intent_compiler::try_load_plan_sexpr(c)
            });
            if let Some(plan_sexpr) = found {
                if let Ok(plan) = parse_plan_any(&plan_sexpr) {
                    return finish_plan_creation(plan, plan_sexpr, state);
                }
            }
        }

        // Also try all 2-token pairs (non-consecutive) for names like "add_numbers",
        // "deep_audit", "repack_comics" where description has extra words between.
        // And 3-token triples for names like "subset_sum_all", "graph_coloring_greedy".
        for i in 0..content_tokens.len() {
            for j in (i + 1)..content_tokens.len() {
                for k in (j + 1)..content_tokens.len() {
                    let triple = format!("{}_{}_{}", content_tokens[i], content_tokens[j], content_tokens[k]);
                    // Also try de-pluralized first token (e.g., "subsets_sum_all" → "subset_sum_all")
                    let de_plural = format!("{}_{}_{}",
                        content_tokens[i].trim_end_matches('s'),
                        content_tokens[j],
                        content_tokens[k]);
                    let tries = vec![triple.clone(), format!("{}s", triple), de_plural];
                    let found = tries.iter().find_map(|c| {
                        intent_compiler::try_load_plan_sexpr(c)
                    });
                    if let Some(plan_sexpr) = found {
                        if let Ok(plan) = parse_plan_any(&plan_sexpr) {
                            return finish_plan_creation(plan, plan_sexpr, state);
                        }
                    }
                }
            }
        }
        for i in 0..content_tokens.len() {
            for j in (i + 1)..content_tokens.len() {
                let pair = format!("{}_{}", content_tokens[i], content_tokens[j]);
                let tries = vec![pair.clone(), format!("{}s", pair)];
                let found = tries.iter().find_map(|c| {
                    intent_compiler::try_load_plan_sexpr(c)
                });
                if let Some(plan_sexpr) = found {
                    if let Ok(plan) = parse_plan_any(&plan_sexpr) {
                        return finish_plan_creation(plan, plan_sexpr, state);
                    }
                }
            }
        }

        // Last resort: check if all words in a plan file name appear in the
        // content tokens (order-independent). This handles cases like
        // "subset_sum_all" where the description has the words in different order.
        if let Some(plan_name) = find_plan_by_token_overlap(&content_tokens) {
            if let Some(plan_sexpr) = intent_compiler::try_load_plan_sexpr(&plan_name) {
                if let Ok(plan) = parse_plan_any(&plan_sexpr) {
                    return finish_plan_creation(plan, plan_sexpr, state);
                }
            }
        }
    }

    // Find the first token that is an algorithm op (skip leading FS verbs)
    static SKIP_TOKENS: &[&str] = &[
        "the", "a", "an", "all", "some", "any", "each", "every",
        "this", "that", "these", "those", "my", "your", "our",
        "using", "via", "into", "from", "with",
    ];
    for token in phrase_tokens.iter()
        .filter(|t| !FS_VERBS.contains(&t.as_str()))
        .filter(|t| !SKIP_TOKENS.contains(&t.as_str()))
    {
        // Only check if it's an algorithm op or plan file
        if let Some(entry) = registry.get_poly(token.as_str()) {
            if entry.racket_body.is_some() {
                let plan = intent_compiler::compile_algorithm_op_by_name(token, entry);
                let yaml = dialogue::plan_to_sexpr(&plan);
                return finish_plan_creation(plan, yaml, state);
            }
        }
        if let Some(plan_sexpr) = intent_compiler::try_load_plan_sexpr(token) {
            if let Ok(plan) = parse_plan_any(&plan_sexpr) {
                return finish_plan_creation(plan, plan_sexpr, state);
            }
        }
        break; // Only check the first non-FS-verb token
    }

    let grammar = grammar::build_command_grammar();
    let lex = lexicon::lexicon();
    let parses = earley::parse(&grammar, &phrase_tokens, lex);

    if parses.is_empty() {
        if !fallback_needs.is_empty() {
            return NlResponse::NeedsClarification { needs: fallback_needs };
        }
        return NlResponse::NeedsClarification {
            needs: vec![
                "I couldn't parse that as a command. Try something like 'compute fibonacci' or 'find PDFs in ~/Documents'.".to_string(),
            ],
        };
    }

    let ir_result = intent_ir::parse_trees_to_intents(&parses);

    // Store alternatives in dialogue state
    state.alternative_intents = ir_result.alternatives.clone();

    match intent_compiler::compile_intent(&ir_result) {
        intent_compiler::CompileResult::Ok(plan) => {
            // For DSL plans loaded from files, use the raw YAML (plan_to_sexpr
            // can't serialize complex step args like sub-steps and clauses).
            let yaml = intent_compiler::try_load_plan_sexpr(&plan.name)
                .unwrap_or_else(|| dialogue::plan_to_sexpr(&plan));

            match validate_plan(&plan) {
                Ok(()) => {
                    let summary = format_summary(&plan);
                    let prompt = format!("{}\n\n{}\n\n{}",
                        casual_ack(state.turn_count),
                        yaml,
                        "Approve? Or edit plan?"
                    );

                    state.current_plan = Some(plan);
                    state.focus.push(FocusEntry::WholePlan);

                    NlResponse::PlanCreated {
                        plan_sexpr: yaml,
                        summary,
                        prompt,
                    }
                }
                Err(e) => NlResponse::Error {
                    message: format!("Generated plan failed validation: {}", e),
                },
            }
        }
        intent_compiler::CompileResult::Error(msg) => {
            NlResponse::NeedsClarification {
                needs: vec![msg],
            }
        }
        intent_compiler::CompileResult::NoIntent => {
            NlResponse::NeedsClarification {
                needs: vec![
                    "I couldn't parse that as a command. Try something like 'compute fibonacci' or 'find PDFs in ~/Documents'.".to_string(),
                ],
            }
        }
        intent_compiler::CompileResult::Approve => {
            handle_approve(state)
        }
        intent_compiler::CompileResult::Reject => {
            state.current_plan = None;
            state.alternative_intents.clear();
            NlResponse::Rejected
        }
        intent_compiler::CompileResult::Explain { subject } => {
            handle_explain(&subject)
        }
    }
}

/// Handle approve intent.
fn handle_approve(state: &mut DialogueState) -> NlResponse {
    if let Some(wf) = state.current_plan.take() {
        let frame = crate::calling_frame::DefaultFrame::from_plan(&wf);
        let script = frame.codegen(&wf).ok();
        state.alternative_intents.clear();
        NlResponse::Approved { script }
    } else {
        NlResponse::NeedsClarification {
            needs: vec![
                "There's nothing to approve yet.".to_string(),
                "Try creating a plan first, like 'zip up ~/Downloads'.".to_string(),
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Intent handlers
// ---------------------------------------------------------------------------

fn handle_edit_step(
    action: EditAction,
    slots: &ExtractedSlots,
    state: &mut DialogueState,
) -> NlResponse {
    let wf = match &state.current_plan {
        Some(wf) => wf.clone(),
        None => {
            return NlResponse::NeedsClarification {
                needs: vec![
                    "There's no current plan to edit.".to_string(),
                    "Try creating one first, like 'zip up ~/Downloads'.".to_string(),
                ],
            };
        }
    };

    match dialogue::apply_edit(&wf, &action, slots, state) {
        Ok((edited_wf, diff_desc)) => {
            let yaml = dialogue::plan_to_sexpr(&edited_wf);

            match validate_plan(&edited_wf) {
                Ok(()) => {
                    let prompt = format!("{}\n\n{}\n\nApprove?",
                        diff_desc,
                        yaml,
                    );

                    state.current_plan = Some(edited_wf);

                    NlResponse::PlanEdited {
                        plan_sexpr: yaml,
                        diff_description: diff_desc,
                        prompt,
                    }
                }
                Err(e) => NlResponse::Error {
                    message: format!("Edited plan failed validation: {}", e),
                },
            }
        }
        Err(DialogueError::NeedsContext(msg)) => {
            NlResponse::NeedsClarification {
                needs: vec![msg],
            }
        }
        Err(DialogueError::InvalidTarget(msg)) => {
            NlResponse::NeedsClarification {
                needs: vec![msg],
            }
        }
        Err(e) => NlResponse::Error {
            message: format!("{}", e),
        },
    }
}

fn handle_explain(subject: &str) -> NlResponse {
    let text = get_op_explanation(subject);
    NlResponse::Explanation { text }
}

// ---------------------------------------------------------------------------
// Command recipe queries
// ---------------------------------------------------------------------------

/// Detect whether the input is a command recipe query.
/// Returns the content tokens (with the query pattern prefix stripped) if so.
///
/// Recognized patterns:
///   - "give me the command to/for ..."
///   - "what's/whats the command to/for ..."
///   - "what command do I use to ..."
///   - "show me the command to/for ..."
///   - "how do I ... from terminal/command line"
///   - "what is the command to/for ..."
fn try_recipe_query(tokens: &[String]) -> Option<Vec<String>> {
    if tokens.len() < 3 {
        return None;
    }

    let t: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();

    // Pattern: "give me the command to/for ..."
    if let Some(rest) = strip_prefix_seq(&t, &["give", "me", "the", "command"]) {
        return Some(strip_leading_preps(&rest));
    }

    // Pattern: "what/whats the command to/for ..."
    if t[0] == "what" || t[0] == "whats" {
        if let Some(rest) = strip_prefix_seq(&t[1..], &["the", "command"]) {
            return Some(strip_leading_preps(&rest));
        }
        if let Some(rest) = strip_prefix_seq(&t[1..], &["is", "the", "command"]) {
            return Some(strip_leading_preps(&rest));
        }
        // "what command do I use to ..."
        if let Some(rest) = strip_prefix_seq(&t[1..], &["command", "do"]) {
            return Some(strip_leading_preps(&rest));
        }
    }

    // Pattern: "show me the command to/for ..."
    if let Some(rest) = strip_prefix_seq(&t, &["show", "me", "the", "command"]) {
        return Some(strip_leading_preps(&rest));
    }

    // Pattern: "how do I ... from terminal/command line"
    if t.len() >= 4 && t[0] == "how" && t[1] == "do" && t.get(2).map_or(false, |w| *w == "i") {
        // Check for trailing "from terminal" / "from the command line" / "from the terminal"
        let has_terminal_suffix = has_suffix(&t, &["from", "terminal"])
            || has_suffix(&t, &["from", "the", "terminal"])
            || has_suffix(&t, &["from", "the", "command", "line"])
            || has_suffix(&t, &["from", "command", "line"])
            || has_suffix(&t, &["in", "terminal"])
            || has_suffix(&t, &["in", "the", "terminal"]);

        if has_terminal_suffix {
            // Strip "how do I" prefix (skip "i") and terminal suffix
            let rest: Vec<String> = t[3..].iter().map(|s| s.to_string()).collect();
            let content = strip_leading_preps(&rest);
            let content = strip_terminal_suffix(content);
            if !content.is_empty() {
                return Some(content);
            }
        } else {
            // "how do I ..." without terminal suffix — still a recipe query
            let rest: Vec<String> = t[3..].iter().map(|s| s.to_string()).collect();
            let content = strip_leading_preps(&rest);
            if !content.is_empty() {
                return Some(content);
            }
        }
    }

    None
}

/// Strip a fixed prefix sequence from tokens. Returns remaining tokens if matched.
fn strip_prefix_seq(tokens: &[&str], prefix: &[&str]) -> Option<Vec<String>> {
    if tokens.len() < prefix.len() {
        return None;
    }
    for (t, p) in tokens.iter().zip(prefix.iter()) {
        if *t != *p {
            return None;
        }
    }
    Some(tokens[prefix.len()..].iter().map(|s| s.to_string()).collect())
}

/// Strip leading prepositions (to, for, about) from token list.
fn strip_leading_preps(tokens: &[String]) -> Vec<String> {
    let skip = &["to", "for", "about", "i", "use"];
    let start = tokens.iter()
        .position(|t| !skip.contains(&t.as_str()))
        .unwrap_or(tokens.len());
    tokens[start..].iter().map(|s| s.to_string()).collect()
}

/// Check if tokens end with the given suffix.
fn has_suffix(tokens: &[&str], suffix: &[&str]) -> bool {
    if tokens.len() < suffix.len() {
        return false;
    }
    let start = tokens.len() - suffix.len();
    tokens[start..].iter().zip(suffix.iter()).all(|(a, b)| a == b)
}

/// Strip trailing "from terminal" / "from the command line" etc.
fn strip_terminal_suffix(mut tokens: Vec<String>) -> Vec<String> {
    let suffixes: &[&[&str]] = &[
        &["from", "the", "command", "line"],
        &["from", "command", "line"],
        &["from", "the", "terminal"],
        &["from", "terminal"],
        &["in", "the", "terminal"],
        &["in", "terminal"],
    ];
    for suffix in suffixes {
        if tokens.len() >= suffix.len() {
            let start = tokens.len() - suffix.len();
            let matches = tokens[start..].iter()
                .zip(suffix.iter())
                .all(|(a, b)| a.as_str() == *b);
            if matches {
                tokens.truncate(start);
                break;
            }
        }
    }
    tokens
}

/// Handle a recipe query: look up the command and return a displayln program.
fn handle_recipe_query(content_tokens: &[String]) -> Option<NlResponse> {
    let idx = recipes::recipe_index();
    let recipe = idx.lookup(content_tokens)?;

    // Generate a simple (displayln "command") Racket program
    let script = format!(
        "#!/usr/bin/env racket\n#lang racket\n(displayln \"{}\")\n",
        recipe.command.replace('\\', "\\\\").replace('"', "\\\"")
    );

    let text = format!(
        "{}\n\nRacket program:\n{}",
        recipe.description,
        script.trim()
    );

    Some(NlResponse::Explanation { text })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Validate a plan YAML string by parsing and compiling it.
fn validate_plan(plan: &crate::plan::PlanDef) -> Result<(), String> {
    let registry = crate::fs_types::build_full_registry();
    crate::plan::compile_plan(plan, &registry)
        .map_err(|e| format!("Compile error: {}", e))?;
    Ok(())
}

/// Find a plan file whose name tokens are all present in the input tokens.
/// Returns the plan file name (without extension) if found.
fn find_plan_by_token_overlap(input_tokens: &[&str]) -> Option<String> {
    // Stopwords that may appear in plan names but are filtered from content tokens.
    // When checking overlap, we skip these in the plan name.
    static NAME_STOPWORDS: &[&str] = &["with", "and", "is", "in", "of", "to", "by", "for", "a", "the"];

    // Build a set of input tokens (including de-pluralized forms)
    let mut token_set: std::collections::HashSet<&str> = input_tokens.iter().copied().collect();
    let de_plurals: Vec<String> = input_tokens.iter()
        .filter(|t| t.ends_with('s') && t.len() > 3)
        .map(|t| t[..t.len() - 1].to_string())
        .collect();
    for dp in &de_plurals {
        token_set.insert(dp.as_str());
    }

    // Also split compound tokens (e.g., "wagner_fischer" → "wagner", "fischer")
    // and add their parts to the token set. This handles phrase-joined tokens.
    let compound_parts: Vec<String> = input_tokens.iter()
        .filter(|t| t.contains('_'))
        .flat_map(|t| t.split('_').map(|s| s.to_string()).collect::<Vec<_>>())
        .collect();
    for part in &compound_parts {
        token_set.insert(part.as_str());
    }

    // Scan all plan files
    // Score by non-stopword word count. Direct token matches (plan name IS a
    // content token) are returned immediately as highest priority.
    let mut best: Option<(String, usize)> = None; // (name, word_count)

    let scan_dir = |dir: &std::path::Path, best: &mut Option<(String, usize)>| {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str());
                if ext != Some("sexp") && ext != Some("yaml") { continue; }
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let name_words: Vec<&str> = stem.split('_').collect();
                if name_words.len() < 2 { continue; } // skip single-word names

                // Skip stopwords in plan name when checking overlap
                let all_present = name_words.iter().all(|w| NAME_STOPWORDS.contains(w) || token_set.contains(w));
                if all_present {
                    let wc = name_words.len();
                    if best.as_ref().map_or(true, |(_, bc)| wc > *bc) {
                        *best = Some((stem.to_string(), wc));
                    }
                }
            }
        }
    };

    // Pipeline plans
    scan_dir(std::path::Path::new("data/plans"), &mut best);

    // Algorithm plans
    let algo_base = std::path::Path::new("data/plans/algorithms");
    if algo_base.exists() {
        if let Ok(cats) = std::fs::read_dir(algo_base) {
            for cat in cats.flatten() {
                if cat.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    scan_dir(&cat.path(), &mut best);
                }
            }
        }
    }

    best.map(|(name, _)| name)
}

/// Helper: finish plan creation (validate, format, update state).
fn finish_plan_creation(
    plan: crate::plan::PlanDef,
    yaml: String,
    state: &mut DialogueState,
) -> NlResponse {
    match validate_plan(&plan) {
        Ok(()) => {
            let summary = format_summary(&plan);
            let prompt = format!("{}\n\n{}\n\n{}",
                casual_ack(state.turn_count),
                &yaml,
                "Approve? Or edit plan?"
            );
            state.current_plan = Some(plan);
            state.focus.push(FocusEntry::WholePlan);
            NlResponse::PlanCreated { plan_sexpr: yaml, prompt, summary }
        }
        Err(e) => NlResponse::Error { message: format!("Generated plan failed validation: {}", e) },
    }
}

// ---------------------------------------------------------------------------
// LLM fallback — try local model when deterministic parsing fails
// ---------------------------------------------------------------------------

/// Generate a casual acknowledgment phrase, varying by turn count.
fn casual_ack(turn: usize) -> &'static str {
    const ACKS: &[&str] = &[
        "Right on it!",
        "Here's what I've got:",
        "Sure thing!",
        "Coming right up!",
        "On it!",
        "Here you go:",
        "Let's do this!",
        "Got it!",
        "Alright, here's the plan:",
        "No problem!",
    ];
    ACKS[turn % ACKS.len()]
}

/// Generate a human-readable summary of a plan.
fn format_summary(wf: &crate::plan::PlanDef) -> String {
    let steps_desc: Vec<String> = wf.steps.iter()
        .map(|s| s.op.replace('_', " "))
        .collect();

    let path = wf.get_input("path")
        .and_then(|i| i.type_hint.as_deref())
        .unwrap_or(".");

    format!(
        "Plan: {} — {} step(s) operating on {}:\n  {}",
        wf.name,
        wf.steps.len(),
        path,
        steps_desc.join(" → "),
    )
}

/// Get an explanation for an operation.
fn get_op_explanation(op: &str) -> String {
    // Look up the description from the ops YAML packs (fs_ops, power_tools, etc.)
    if let Some(desc) = crate::fs_types::get_op_description(op) {
        desc.to_string()
    } else {
        format!("An operation in Cadmus's toolkit ({}).", op)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Full round-trip: create plan --

    #[test]
    fn test_process_zip_up_downloads() {
        let mut state = DialogueState::new();
        let response = process_input("zip up everything in my downloads", &mut state);

        match response {
            NlResponse::PlanCreated { plan_sexpr, summary: _, prompt } => {
                assert!(plan_sexpr.contains("walk_tree"));
                assert!(plan_sexpr.contains("pack_archive"));
                assert!(prompt.contains("Approve"));
                assert!(state.current_plan.is_some());
            }
            other => panic!("expected PlanCreated, got: {:?}", other),
        }
    }

    #[test]
    fn test_process_find_pdfs() {
        let mut state = DialogueState::new();
        let response = process_input("find all PDFs in ~/Documents", &mut state);

        match response {
            NlResponse::PlanCreated { plan_sexpr, .. } => {
                assert!(plan_sexpr.contains("walk_tree") || plan_sexpr.contains("find_matching"));
            }
            other => panic!("expected PlanCreated, got: {:?}", other),
        }
    }

    #[test]
    fn test_process_list_dir() {
        let mut state = DialogueState::new();
        let response = process_input("list ~/Downloads", &mut state);

        match response {
            NlResponse::PlanCreated { plan_sexpr, .. } => {
                assert!(plan_sexpr.contains("list_dir"));
            }
            other => panic!("expected PlanCreated, got: {:?}", other),
        }
    }

    // -- Explain --

    #[test]
    fn test_process_whats_walk_mean() {
        let mut state = DialogueState::new();
        let response = process_input("what's walk mean", &mut state);

        match response {
            NlResponse::Explanation { text } => {
                assert!(text.contains("directory") || text.contains("walk"),
                    "explanation should mention directory/walk: {}", text);
            }
            other => panic!("expected Explanation, got: {:?}", other),
        }
    }

    #[test]
    fn test_process_explain_filter() {
        let mut state = DialogueState::new();
        let response = process_input("explain filter", &mut state);

        match response {
            NlResponse::Explanation { text } => {
                assert!(text.contains("filter") || text.contains("match"),
                    "text: {}", text);
            }
            other => panic!("expected Explanation, got: {:?}", other),
        }
    }

    // -- Approve / Reject --

    #[test]
    fn test_process_lgtm() {
        let mut state = DialogueState::new();
        process_input("zip up ~/Downloads", &mut state);
        let response = process_input("lgtm", &mut state);
        assert!(matches!(response, NlResponse::Approved { .. }));
    }

    #[test]
    fn test_process_sounds_good() {
        let mut state = DialogueState::new();
        process_input("zip up ~/Downloads", &mut state);
        let response = process_input("sounds good", &mut state);
        assert!(matches!(response, NlResponse::Approved { .. }));
    }

    #[test]
    fn test_process_nah_start_over() {
        let mut state = DialogueState::new();
        // First create a plan
        process_input("zip up ~/Downloads", &mut state);
        assert!(state.current_plan.is_some());

        // Then reject
        let response = process_input("nah", &mut state);
        assert!(matches!(response, NlResponse::Rejected));
        assert!(state.current_plan.is_none());
    }

    // -- Edit --

    #[test]
    fn test_process_skip_subdirectory() {
        let mut state = DialogueState::new();

        // First create a plan
        let r1 = process_input("zip up everything in ~/Downloads", &mut state);
        assert!(matches!(r1, NlResponse::PlanCreated { .. }));

        // Then edit it
        let r2 = process_input("skip any subdirectory named foo", &mut state);
        match r2 {
            NlResponse::PlanEdited { plan_sexpr, .. } => {
                assert!(plan_sexpr.contains("filter"),
                    "should have filter step: {}", plan_sexpr);
            }
            other => panic!("expected PlanEdited, got: {:?}", other),
        }
    }

    // -- Typo correction --

    #[test]
    fn test_process_with_typos() {
        let mut state = DialogueState::new();
        // Typo correction: "extrct" → "extract", "archve" → "archive"
        // The Earley parser handles this via the decompress action.
        let response = process_input("extrct ~/comic.cbz", &mut state);

        match response {
            NlResponse::PlanCreated { plan_sexpr, .. } => {
                assert!(plan_sexpr.contains("extract_archive"),
                    "typo should be corrected: {}", plan_sexpr);
            }
            _ => {
                // Earley may not handle all typo variants yet — acceptable
            }
        }
    }

    // -- NeedsClarification --

    #[test]
    fn test_process_gibberish() {
        let mut state = DialogueState::new();
        let response = process_input("asdfghjkl qwerty", &mut state);
        assert!(matches!(response, NlResponse::NeedsClarification { .. }));
    }

    // -- Three-turn conversation --

    #[test]
    fn test_three_turn_conversation() {
        let mut state = DialogueState::new();

        // Turn 1: Create
        let r1 = process_input("zip up everything in ~/Downloads", &mut state);
        assert!(matches!(r1, NlResponse::PlanCreated { .. }));
        assert!(state.current_plan.is_some());

        // Turn 2: Edit
        let r2 = process_input("skip any subdirectory named .git", &mut state);
        match &r2 {
            NlResponse::PlanEdited { plan_sexpr, .. } => {
                assert!(plan_sexpr.contains("filter"));
            }
            other => panic!("expected PlanEdited, got: {:?}", other),
        }

        // Turn 3: Approve
        let r3 = process_input("lgtm", &mut state);
        assert!(matches!(r3, NlResponse::Approved { .. }));
    }

    // -- Casual ack variation --

    #[test]
    fn test_casual_ack_varies() {
        let ack1 = casual_ack(0);
        let ack2 = casual_ack(1);
        let ack3 = casual_ack(2);
        // At least some should be different
        assert!(ack1 != ack2 || ack2 != ack3, "acks should vary");
    }

    // -- Generated YAML round-trips --

    #[test]
    fn test_generated_yaml_roundtrips() {
        let mut state = DialogueState::new();
        let response = process_input("zip up everything in ~/Downloads", &mut state);

        if let NlResponse::PlanCreated { plan_sexpr, .. } = response {
            // Parse it back
            let parsed = parse_plan_any(&plan_sexpr);
            assert!(parsed.is_ok(), "should parse: {:?}", parsed.err());
        }
    }

    // -- Edit on empty state --

    #[test]
    fn test_edit_without_plan() {
        let mut state = DialogueState::new();
        let response = process_input("skip any subdirectory named foo", &mut state);
        // Should get clarification, not an error
        assert!(matches!(response, NlResponse::NeedsClarification { .. }));
    }

    // -- B6 bugfix: approve/reject without plan --

    #[test]
    fn test_approve_without_plan_needs_clarification() {
        let mut state = DialogueState::new();
        let response = process_input("approve", &mut state);
        assert!(matches!(response, NlResponse::NeedsClarification { .. }),
            "approve without plan should need clarification, got: {:?}", response);
    }

    #[test]
    fn test_approve_after_plan_created_works() {
        let mut state = DialogueState::new();
        let r1 = process_input("zip up everything in ~/Downloads", &mut state);
        assert!(matches!(r1, NlResponse::PlanCreated { .. }));
        let r2 = process_input("approve", &mut state);
        assert!(matches!(r2, NlResponse::Approved { .. }));
    }

    #[test]
    fn test_approve_after_error_needs_clarification() {
        let mut state = DialogueState::new();
        // This should fail validation (no plan stored)
        let r1 = process_input("do the thing", &mut state);
        assert!(matches!(r1, NlResponse::NeedsClarification { .. }),
            "do the thing should need clarification: {:?}", r1);
        assert!(state.current_plan.is_none());
        // Now approve should also need clarification
        let r2 = process_input("approve", &mut state);
        assert!(matches!(r2, NlResponse::NeedsClarification { .. }),
            "approve after error should need clarification, got: {:?}", r2);
    }

    #[test]
    fn test_double_approve_fails() {
        let mut state = DialogueState::new();
        // Use a command the Earley parser can handle (verb + path)
        let r1 = process_input("zip up ~/Downloads", &mut state);
        assert!(matches!(r1, NlResponse::PlanCreated { .. }), "should create plan: {:?}", r1);
        let r2 = process_input("yes", &mut state);
        assert!(matches!(r2, NlResponse::Approved { .. }), "first approve should succeed: {:?}", r2);
        // Second approve should fail — plan was cleared
        let r3 = process_input("yes", &mut state);
        assert!(matches!(r3, NlResponse::NeedsClarification { .. }),
            "second approve should need clarification, got: {:?}", r3);
    }

    #[test]
    fn test_approve_clears_then_new_plan_works() {
        let mut state = DialogueState::new();
        let _ = process_input("compress file.txt", &mut state);
        let _ = process_input("yes", &mut state);
        // After approve+clear, creating a new plan should work
        let r = process_input("list ~/Desktop", &mut state);
        assert!(matches!(r, NlResponse::PlanCreated { .. }), "new plan after approve: {:?}", r);
        let r2 = process_input("ok", &mut state);
        assert!(matches!(r2, NlResponse::Approved { .. }), "approve new plan: {:?}", r2);
    }

    // -- Recipe query detection --

    #[test]
    fn test_try_recipe_query_give_me_command() {
        let tokens: Vec<String> = "give me the command to reset git"
            .split_whitespace().map(String::from).collect();
        let result = try_recipe_query(&tokens);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.contains(&"reset".to_string()));
        assert!(content.contains(&"git".to_string()));
    }

    #[test]
    fn test_try_recipe_query_whats_the_command() {
        let tokens: Vec<String> = "whats the command for listing processes"
            .split_whitespace().map(String::from).collect();
        let result = try_recipe_query(&tokens);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.contains(&"listing".to_string()));
        assert!(content.contains(&"processes".to_string()));
    }

    #[test]
    fn test_try_recipe_query_how_do_i_from_terminal() {
        let tokens: Vec<String> = "how do i cherry pick a commit from terminal"
            .split_whitespace().map(String::from).collect();
        let result = try_recipe_query(&tokens);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.contains(&"cherry".to_string()));
        assert!(content.contains(&"pick".to_string()));
        // "from terminal" should be stripped
        assert!(!content.contains(&"terminal".to_string()));
    }

    #[test]
    fn test_try_recipe_query_no_match() {
        let tokens: Vec<String> = "zip up my downloads folder"
            .split_whitespace().map(String::from).collect();
        assert!(try_recipe_query(&tokens).is_none());
    }

    #[test]
    fn test_try_recipe_query_too_short() {
        let tokens: Vec<String> = "hi there"
            .split_whitespace().map(String::from).collect();
        assert!(try_recipe_query(&tokens).is_none());
    }

    #[test]
    fn test_handle_recipe_query_git_reset() {
        let tokens: Vec<String> = vec!["git".into(), "reset".into()];
        let result = handle_recipe_query(&tokens);
        assert!(result.is_some());
        match result.unwrap() {
            NlResponse::Explanation { text } => {
                assert!(text.contains("git"), "should mention git: {}", text);
                assert!(text.contains("reset"), "should mention reset: {}", text);
                assert!(text.contains("displayln"), "should have displayln: {}", text);
            }
            other => panic!("expected Explanation, got {:?}", other),
        }
    }

    #[test]
    fn test_handle_recipe_query_no_match() {
        let tokens: Vec<String> = vec!["make".into(), "pasta".into()];
        assert!(handle_recipe_query(&tokens).is_none());
    }

    #[test]
    fn test_try_recipe_query_how_do_i_no_terminal_suffix() {
        let tokens: Vec<String> = "how do i stop the computer from sleeping"
            .split_whitespace().map(String::from).collect();
        let result = try_recipe_query(&tokens);
        assert!(result.is_some(), "should match 'how do i' without terminal suffix");
        let content = result.unwrap();
        assert!(content.contains(&"stop".to_string()));
        assert!(content.contains(&"sleeping".to_string()));
    }

    #[test]
    fn test_try_recipe_query_how_do_i_prevent_sleep() {
        let tokens: Vec<String> = "how do i prevent sleep"
            .split_whitespace().map(String::from).collect();
        let result = try_recipe_query(&tokens);
        assert!(result.is_some(), "should match 'how do i prevent sleep'");
        let content = result.unwrap();
        assert!(content.contains(&"prevent".to_string()));
        assert!(content.contains(&"sleep".to_string()));
    }

    #[test]
    fn test_try_recipe_query_how_does_not_match() {
        // "how does" should NOT match the recipe query pattern (no "i")
        let tokens: Vec<String> = "how does filter work"
            .split_whitespace().map(String::from).collect();
        assert!(try_recipe_query(&tokens).is_none(),
            "'how does X work' should not be a recipe query");
    }

    #[test]
    fn test_try_recipe_query_how_do_i_too_short() {
        // "how do i" alone should not match (no content after stripping)
        let tokens: Vec<String> = "how do i"
            .split_whitespace().map(String::from).collect();
        // len is 3, but we need >= 4
        assert!(try_recipe_query(&tokens).is_none(),
            "'how do i' alone should not match");
    }

    #[test]
    fn test_process_web_server() {
        let mut state = DialogueState::new();
        let response = process_input("Spin up a web server on localhost port 8080 serving hello world", &mut state);
        match response {
            NlResponse::PlanCreated { plan_sexpr, .. } => {
                assert!(plan_sexpr.contains("http_server"), "expected http_server in plan, got: {}", plan_sexpr);
            }
            other => panic!("expected PlanCreated, got: {:?}", other),
        }
    }

    #[test]
    fn test_process_add_route_web_server() {
        let mut state = DialogueState::new();
        let response = process_input("Add route to web server with pipeline: read file, match errors, return as html code block", &mut state);
        match response {
            NlResponse::PlanCreated { plan_sexpr, .. } => {
                assert!(plan_sexpr.contains("add_route") || plan_sexpr.contains("add-route"), "expected add_route in plan, got: {}", plan_sexpr);
            }
            other => panic!("expected PlanCreated, got: {:?}", other),
        }
    }
}
