//! Intent IR — the structured intermediate representation between
//! Earley parse trees and PlanDef compilation.
//!
//! The Intent IR captures the *what* without the *how*:
//! - Abstract actions (select, order, compress) — not cadmus ops
//! - Concept labels (comic_issue_archive) — not file patterns
//! - Semantic selectors (scope, constraints) — not step params
//!
//! The IR is compiled to PlanDef in a separate phase (intent_compiler).

use std::collections::HashMap;

use crate::nl::earley::{ParseNode, RankedParse};
use crate::nl::lexicon::{self, Lexicon};

// ---------------------------------------------------------------------------
// Intent IR types
// ---------------------------------------------------------------------------

/// The top-level intent intermediate representation.
#[derive(Debug, Clone)]
pub struct IntentIR {
    /// The expected output type (e.g., "Collection<File>").
    pub output: String,

    /// Named inputs with type and selector information.
    pub inputs: Vec<IRInput>,

    /// Abstract processing steps.
    pub steps: Vec<IRStep>,

    /// Constraints on the result.
    pub constraints: Vec<String>,

    /// Acceptance criteria.
    pub acceptance: Vec<String>,

    /// The score of the parse that produced this IR.
    pub score: f64,
}

/// An input to the intent.
#[derive(Debug, Clone)]
pub struct IRInput {
    /// Input identifier (e.g., "path", "source").
    pub name: String,
    /// Type expression (e.g., "Collection<File>").
    pub type_expr: String,
    /// Selector: how to obtain this input.
    pub selector: IRSelector,
}

/// A selector expression for an input.
#[derive(Debug, Clone)]
pub struct IRSelector {
    /// Scope: where to look.
    pub scope: Option<IRScope>,
}

/// Scope for a selector.
#[derive(Debug, Clone)]
pub struct IRScope {
    /// Directory path.
    pub dir: String,
    /// Whether to recurse into subdirectories.
    pub recursive: bool,
}

/// An abstract processing step.
#[derive(Debug, Clone)]
pub struct IRStep {
    /// The abstract action verb (e.g., "select", "order", "compress").
    pub action: String,
    /// Input entity IDs or step IDs.
    pub input_refs: Vec<String>,
    /// Output entity ID.
    pub output_ref: String,
    /// Action parameters.
    pub params: HashMap<String, String>,
}

/// The kind of utterance parsed.
#[derive(Debug, Clone, PartialEq)]
pub enum UtteranceKind {
    /// A command to create a plan.
    Command,
    /// User approves the current plan.
    Approve,
    /// User rejects the current plan.
    Reject,
    /// User asks for an explanation of something.
    Explain { subject: String },
}

/// Result of converting parse trees to Intent IRs.
#[derive(Debug, Clone)]
pub struct IntentIRResult {
    /// The primary (highest-scored) intent IR.
    pub primary: Option<IntentIR>,
    /// Alternative interpretations, ranked by score.
    pub alternatives: Vec<IntentIR>,
    /// The kind of utterance (Command, Approve, Reject, Explain).
    pub kind: UtteranceKind,
}

// ---------------------------------------------------------------------------
// Parse tree → Intent IR conversion
// ---------------------------------------------------------------------------

/// Convert ranked parse trees into Intent IRs.
///
/// Takes the output of the Earley parser and produces structured intents
/// with semantic resolution (concept labels, paths, orderings).
pub fn parse_trees_to_intents(parses: &[RankedParse]) -> IntentIRResult {
    if parses.is_empty() {
        return IntentIRResult {
            primary: None,
            alternatives: Vec::new(),
            kind: UtteranceKind::Command,
        };
    }

    let lex = lexicon::lexicon();
    let mut intents: Vec<IntentIR> = Vec::new();

    for ranked in parses {
        if let Some(ir) = tree_to_intent(&ranked.tree, ranked.score, lex) {
            intents.push(ir);
        }
    }

    // Sort by score descending
    intents.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    let primary = intents.first().cloned();
    let alternatives = if intents.len() > 1 {
        intents[1..].to_vec()
    } else {
        Vec::new()
    };

    IntentIRResult { primary, alternatives, kind: UtteranceKind::Command }
}

/// Convert a single parse tree to an Intent IR.
fn tree_to_intent(tree: &ParseNode, score: f64, lex: &Lexicon) -> Option<IntentIR> {
    // Extract verb (action)
    let verb_node = tree.find("Verb")?;
    let verb_token = verb_node.children().first()?.token()?;
    let verb_info = lex.verbs.get(&verb_token.to_lowercase())?;
    let action = &verb_info.action;

    // Extract object (concept)
    let object_concept = extract_object_concept(tree, lex);

    // Extract location modifier
    let location = extract_location(tree, lex);

    // Extract object path (e.g., "~/comic.cbz" in "extract ~/comic.cbz")
    let object_path = extract_object_path(tree);

    // If no explicit location, check if the object itself is a path_noun
    let location = location.or_else(|| extract_object_as_location(tree, lex));

    // Extract ordering modifier
    let ordering = extract_ordering(tree, lex);

    // Extract pattern modifier
    let pattern = extract_pattern(tree);

    // Build the IR
    let mut inputs = Vec::new();
    let mut steps = Vec::new();
    let mut constraints = Vec::new();
    let mut acceptance = Vec::new();

    // Determine output type based on action
    let output = match action.as_str() {
        "select" | "enumerate" | "retrieve" | "traverse" | "search_text" => "Collection<File>".to_string(),
        "compress" | "decompress" => "File".to_string(),
        "order" => "Collection<File>".to_string(),
        "count" => "Number".to_string(),
        "deduplicate" => "Collection<File>".to_string(),
        "copy" | "move" | "rename" | "delete" => "File".to_string(),
        "read" => "Text".to_string(),
        "write" => "File".to_string(),
        "compare" => "Diff".to_string(),
        "checksum" => "Hash".to_string(),
        _ => "Collection<File>".to_string(),
    };

    // Build input from location or object path
    let dir = object_path.as_deref()
        .or(location.as_deref())
        .unwrap_or(".");
    let input_type = if object_concept.is_some() {
        "Collection<File>".to_string()
    } else {
        "Collection<File>".to_string()
    };

    inputs.push(IRInput {
        name: "path".to_string(),
        type_expr: input_type,
        selector: IRSelector {
            scope: Some(IRScope {
                dir: dir.to_string(),
                recursive: true,
            }),
        },
    });

    // Build steps based on action + modifiers
    let mut step_counter = 0;
    let mut last_ref = "universe".to_string();

    // Step 1: selection/filtering based on concept
    // Skip generic concepts that don't need filtering
    let generic_concepts = ["file", "directory", "all_files"];
    let filterable_concept = object_concept.as_ref()
        .filter(|c| !generic_concepts.contains(&c.as_str()));

    if let Some(concept) = &filterable_concept {
        step_counter += 1;
        let out_ref = format!("step_{}", step_counter);
        let mut params = HashMap::new();
        params.insert("where".to_string(), format!("kind: \"{}\"", concept));

        // Preserve the original verb action so the compiler can use it
        // for concept-ops dispatch (e.g., "list containers" → enumerate + docker_container).
        params.insert("verb_action".to_string(), action.clone());

        // If we have a pattern, add it
        if let Some(ref pat) = pattern {
            params.insert("pattern".to_string(), pat.clone());
        }

        steps.push(IRStep {
            action: "select".to_string(),
            input_refs: vec![last_ref.clone()],
            output_ref: out_ref.clone(),
            params,
        });
        last_ref = out_ref;

        // Add acceptance criterion
        acceptance.push(format!("all_results_are_kind: {}", concept));
    } else if let Some(ref pat) = pattern {
        // Pattern without concept
        step_counter += 1;
        let out_ref = format!("step_{}", step_counter);
        let mut params = HashMap::new();
        params.insert("pattern".to_string(), pat.clone());

        steps.push(IRStep {
            action: "select".to_string(),
            input_refs: vec![last_ref.clone()],
            output_ref: out_ref.clone(),
            params,
        });
        last_ref = out_ref;
    }

    // If the primary action is NOT select (e.g., compress, decompress, order),
    // add it as a step
    match action.as_str() {
        "select" | "enumerate" | "retrieve" | "traverse" => {
            // Already handled by the select step above, or it's the primary action
            if steps.is_empty() {
                // No concept/pattern — the action itself is the step
                step_counter += 1;
                let out_ref = format!("step_{}", step_counter);
                steps.push(IRStep {
                    action: action.clone(),
                    input_refs: vec![last_ref.clone()],
                    output_ref: out_ref.clone(),
                    params: HashMap::new(),
                });
                last_ref = out_ref;
            }
        }
        "compress" => {
            step_counter += 1;
            let out_ref = format!("step_{}", step_counter);
            steps.push(IRStep {
                action: "compress".to_string(),
                input_refs: vec![last_ref.clone()],
                output_ref: out_ref.clone(),
                params: HashMap::new(),
            });
            last_ref = out_ref;
        }
        "decompress" => {
            step_counter += 1;
            let out_ref = format!("step_{}", step_counter);
            steps.push(IRStep {
                action: "decompress".to_string(),
                input_refs: vec![last_ref.clone()],
                output_ref: out_ref.clone(),
                params: HashMap::new(),
            });
            last_ref = out_ref;
        }
        "order" => {
            // Order is handled below as an ordering modifier
            if ordering.is_none() {
                // Bare "sort" without ordering — add default
                step_counter += 1;
                let out_ref = format!("step_{}", step_counter);
                let mut params = HashMap::new();
                params.insert("by".to_string(), "name".to_string());
                params.insert("direction".to_string(), "ascending".to_string());
                steps.push(IRStep {
                    action: "order".to_string(),
                    input_refs: vec![last_ref.clone()],
                    output_ref: out_ref.clone(),
                    params,
                });
                last_ref = out_ref;
            }
        }
        "count" => {
            step_counter += 1;
            let out_ref = format!("step_{}", step_counter);
            steps.push(IRStep {
                action: "count".to_string(),
                input_refs: vec![last_ref.clone()],
                output_ref: out_ref.clone(),
                params: HashMap::new(),
            });
            last_ref = out_ref;
        }
        other => {
            // Generic action
            step_counter += 1;
            let out_ref = format!("step_{}", step_counter);
            steps.push(IRStep {
                action: other.to_string(),
                input_refs: vec![last_ref.clone()],
                output_ref: out_ref.clone(),
                params: HashMap::new(),
            });
            last_ref = out_ref;
        }
    }

    // Add ordering step if present
    if let Some((field, direction)) = ordering {
        step_counter += 1;
        let out_ref = format!("step_{}", step_counter);
        let mut params = HashMap::new();
        params.insert("by".to_string(), field.clone());
        params.insert("direction".to_string(), direction.clone());

        steps.push(IRStep {
            action: "order".to_string(),
            input_refs: vec![last_ref.clone()],
            output_ref: out_ref.clone(),
            params,
        });
        let _last = out_ref;

        // Add acceptance criterion
        acceptance.push(format!("results_sorted_by: {}_{}", field,
            if direction == "descending" { "desc" } else { "asc" }));
        constraints.push("stable_order".to_string());
        constraints.push("deterministic".to_string());
    }

    Some(IntentIR {
        output,
        inputs,
        steps,
        constraints,
        acceptance,
        score,
    })
}

// ---------------------------------------------------------------------------
// Parse tree extraction helpers
// ---------------------------------------------------------------------------

/// Extract the object concept from the parse tree.
/// Returns the concept label (e.g., "comic_issue_archive") or None.
fn extract_object_concept(tree: &ParseNode, lex: &Lexicon) -> Option<String> {
    let obj = tree.find("Object")?;
    let np = obj.find("NounPhrase");

    if let Some(np_node) = np {
        // Collect all noun tokens in the NounPhrase
        let tokens: Vec<&str> = np_node.leaf_tokens();

        // Try bigram first (e.g., "comic books")
        if tokens.len() >= 2 {
            let bigram = format!("{} {}", tokens[tokens.len() - 2], tokens[tokens.len() - 1]);
            if let Some(info) = lex.nouns.get(&bigram.to_lowercase()) {
                return Some(info.concept.clone());
            }
        }

        // Try each token as a noun
        for token in tokens.iter().rev() {
            if let Some(info) = lex.nouns.get(&token.to_lowercase()) {
                return Some(info.concept.clone());
            }
        }
    }

    // Object might be a direct path or pattern — no concept
    None
}

/// Extract the location path from the parse tree.
/// Returns the resolved filesystem path (e.g., "~/Downloads") or None.
fn extract_location(tree: &ParseNode, lex: &Lexicon) -> Option<String> {
    let loc = tree.find("LocationMod")?;
    let tokens: Vec<&str> = loc.leaf_tokens();

    // Look for explicit path tokens (~/..., /...)
    for token in &tokens {
        if token.starts_with("~/") || token.starts_with('/') || token.starts_with("$HOME") {
            return Some(token.to_string());
        }
    }

    // Look for path_noun tokens
    for token in &tokens {
        if let Some(info) = lex.path_nouns.get(&token.to_lowercase()) {
            return Some(info.path.clone());
        }
    }

    // Fallback: look for any noun that might be a directory alias
    for token in &tokens {
        if let Some(info) = lex.path_nouns.get(&token.to_lowercase()) {
            return Some(info.path.clone());
        }
    }

    None
}

/// Extract ordering information from the parse tree.
/// Returns (field, direction) or None.
fn extract_ordering(tree: &ParseNode, lex: &Lexicon) -> Option<(String, String)> {
    let order = tree.find("OrderMod")?;
    let tokens: Vec<&str> = order.leaf_tokens();

    // Look up each token in the ordering table
    for token in &tokens {
        if let Some(info) = lex.orderings.get(&token.to_lowercase()) {
            return Some((info.field.clone(), info.direction.clone()));
        }
    }

    None
}

/// Extract pattern from the parse tree.
fn extract_pattern(tree: &ParseNode) -> Option<String> {
    let pat = tree.find("PatternMod")?;
    let tokens: Vec<&str> = pat.leaf_tokens();
    tokens.first().map(|t| t.to_string())
}

/// Check if the Object is a path_noun and extract its path.
/// Used as fallback when there's no explicit LocationMod.
/// E.g., "list desktop" → Object is "desktop" which is a path_noun → ~/Desktop.
fn extract_object_as_location(tree: &ParseNode, lex: &Lexicon) -> Option<String> {
    let obj = tree.find("Object")?;
    let tokens: Vec<&str> = obj.leaf_tokens();

    for token in &tokens {
        if let Some(info) = lex.path_nouns.get(&token.to_lowercase()) {
            return Some(info.path.clone());
        }
    }

    None
}

/// Extract a filesystem path from the Object node.
/// E.g., "extract ~/comic.cbz" → Object is "~/comic.cbz" → Some("~/comic.cbz").
fn extract_object_path(tree: &ParseNode) -> Option<String> {
    let obj = tree.find("Object")?;
    let tokens: Vec<&str> = obj.leaf_tokens();

    for token in &tokens {
        if token.starts_with("~/")
            || token.starts_with('/')
            || token.starts_with("$HOME")
        {
            return Some(token.to_string());
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nl::earley;
    use crate::nl::grammar::build_command_grammar;
    use crate::nl::lexicon::lexicon;

    fn parse_and_convert(input: &str) -> IntentIRResult {
        let grammar = build_command_grammar();
        let lex = lexicon();
        let tokens: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();
        let parses = earley::parse(&grammar, &tokens, lex);
        parse_trees_to_intents(&parses)
    }

    #[test]
    fn test_find_comics_in_downloads_newest_first() {
        let result = parse_and_convert("find comics in my downloads folder newest first");
        let ir = result.primary.expect("should produce an intent");

        assert_eq!(ir.output, "Collection<File>");

        // Should have path input with ~/Downloads
        assert!(!ir.inputs.is_empty());
        let path_input = &ir.inputs[0];
        assert_eq!(path_input.name, "path");
        let scope = path_input.selector.scope.as_ref().expect("should have scope");
        assert_eq!(scope.dir, "~/Downloads");

        // Should have select step with comic_issue_archive
        let select_step = ir.steps.iter().find(|s| s.action == "select");
        assert!(select_step.is_some(), "should have select step: {:?}", ir.steps);
        let select = select_step.unwrap();
        assert!(select.params.get("where").unwrap().contains("comic_issue_archive"),
            "select should filter by comic_issue_archive: {:?}", select.params);

        // Should have order step with modification_time descending
        let order_step = ir.steps.iter().find(|s| s.action == "order");
        assert!(order_step.is_some(), "should have order step: {:?}", ir.steps);
        let order = order_step.unwrap();
        assert_eq!(order.params.get("by"), Some(&"modification_time".to_string()));
        assert_eq!(order.params.get("direction"), Some(&"descending".to_string()));

        // Should have constraints
        assert!(ir.constraints.contains(&"stable_order".to_string()));
        assert!(ir.constraints.contains(&"deterministic".to_string()));

        // Should have acceptance criteria
        assert!(ir.acceptance.iter().any(|a| a.contains("comic_issue_archive")));
        assert!(ir.acceptance.iter().any(|a| a.contains("modification_time")));
    }

    #[test]
    fn test_find_comics_in_downloads() {
        let result = parse_and_convert("find comics in downloads");
        let ir = result.primary.expect("should produce an intent");

        assert_eq!(ir.output, "Collection<File>");
        let scope = ir.inputs[0].selector.scope.as_ref().unwrap();
        assert_eq!(scope.dir, "~/Downloads");

        let select_step = ir.steps.iter().find(|s| s.action == "select").unwrap();
        assert!(select_step.params.get("where").unwrap().contains("comic_issue_archive"));
    }

    #[test]
    fn test_sort_files_newest_first() {
        let result = parse_and_convert("sort files newest first");
        let ir = result.primary.expect("should produce an intent");

        // Primary action is "order" (from verb "sort")
        let order_step = ir.steps.iter().find(|s| s.action == "order");
        assert!(order_step.is_some(), "should have order step: {:?}", ir.steps);
    }

    #[test]
    fn test_zip_up_everything_in_downloads() {
        let result = parse_and_convert("zip up everything in downloads");
        let ir = result.primary.expect("should produce an intent");

        assert_eq!(ir.output, "File"); // compress produces a file
        let compress_step = ir.steps.iter().find(|s| s.action == "compress");
        assert!(compress_step.is_some(), "should have compress step: {:?}", ir.steps);
    }

    #[test]
    fn test_extract_path() {
        let result = parse_and_convert("extract ~/comic.cbz");
        let ir = result.primary.expect("should produce an intent");

        assert_eq!(ir.output, "File");
        let decompress_step = ir.steps.iter().find(|s| s.action == "decompress");
        assert!(decompress_step.is_some(), "should have decompress step: {:?}", ir.steps);
    }

    #[test]
    fn test_list_bare_verb() {
        let result = parse_and_convert("list");
        let ir = result.primary.expect("should produce an intent");

        assert_eq!(ir.output, "Collection<File>");
        assert!(!ir.steps.is_empty());
    }

    #[test]
    fn test_empty_input() {
        let result = parse_and_convert("");
        assert!(result.primary.is_none(), "empty input should produce no intent");
    }

    #[test]
    fn test_gibberish() {
        let result = parse_and_convert("asdfghjkl qwerty");
        assert!(result.primary.is_none(), "gibberish should produce no intent");
    }

    #[test]
    fn test_no_location_defaults_to_current_dir() {
        let result = parse_and_convert("find comics");
        let ir = result.primary.expect("should produce an intent");

        let scope = ir.inputs[0].selector.scope.as_ref().unwrap();
        assert_eq!(scope.dir, ".", "should default to current directory");
    }

    #[test]
    fn test_unresolvable_concept_preserved() {
        // "files" is a generic concept, not a specific type
        let result = parse_and_convert("find files");
        let ir = result.primary.expect("should produce an intent");

        // "files" maps to concept "file" — should be preserved
        let select_step = ir.steps.iter().find(|s| s.action == "select");
        assert!(select_step.is_some());
    }

    #[test]
    fn test_alternatives_preserved() {
        let result = parse_and_convert("find comics in downloads");
        // There might be multiple parses — alternatives should be preserved
        // (even if empty, the structure should be valid)
        assert!(result.primary.is_some());
        // alternatives is a Vec, could be empty or not
    }

    #[test]
    fn test_find_pdfs_in_documents() {
        let result = parse_and_convert("find pdfs in documents");
        let ir = result.primary.expect("should produce an intent");

        let scope = ir.inputs[0].selector.scope.as_ref().unwrap();
        assert_eq!(scope.dir, "~/Documents");

        let select_step = ir.steps.iter().find(|s| s.action == "select").unwrap();
        assert!(select_step.params.get("where").unwrap().contains("pdf_document"));
    }

    #[test]
    fn test_find_photos_by_size() {
        let result = parse_and_convert("find photos largest first");
        let ir = result.primary.expect("should produce an intent");

        let select_step = ir.steps.iter().find(|s| s.action == "select").unwrap();
        assert!(select_step.params.get("where").unwrap().contains("photograph"));

        let order_step = ir.steps.iter().find(|s| s.action == "order").unwrap();
        assert_eq!(order_step.params.get("by"), Some(&"size".to_string()));
        assert_eq!(order_step.params.get("direction"), Some(&"descending".to_string()));
    }

    #[test]
    fn test_find_with_glob_pattern() {
        let result = parse_and_convert("find *.pdf in documents");
        let ir = result.primary.expect("should produce an intent");

        // Should have a select step with pattern
        let select_step = ir.steps.iter().find(|s| s.action == "select");
        assert!(select_step.is_some(), "should have select step: {:?}", ir.steps);
    }

    #[test]
    fn test_score_preserved() {
        let result = parse_and_convert("find comics in downloads");
        let ir = result.primary.expect("should produce an intent");
        assert!(ir.score > 0.0, "score should be positive: {}", ir.score);
    }
}
