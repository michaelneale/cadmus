//! Intent IR → PlanDef compiler.
//!
//! **Program-first**: every NL intent maps to "write a program". The compiler
//! first checks if the action label is a registered op (algorithm atom,
//! filesystem op, etc.) and produces a single-step plan. Filesystem-specific
//! multi-step patterns (select → walk_tree + find_matching, compress →
//! walk_tree + pack_archive, etc.) are handled as special cases.
//!
//! Action label → plan mapping:
//!
//! **Filesystem patterns** (multi-step):
//! - `select` → `walk_tree` + `find_matching` (or `filter`)
//! - `order` → `sort_by`
//! - `compress` → `walk_tree` + `pack_archive`
//! - `decompress` → `extract_archive`
//! - `enumerate` → `list_dir`
//! - `traverse` → `walk_tree`
//! - `count` → `walk_tree` + `count_entries`
//! - `read` → `read_file`
//! - `delete` → `delete`
//! - `copy` → `copy`
//! - `move` → `move_entry`
//! - `rename` → `rename`
//! - `deduplicate` → `walk_tree` + `find_duplicates`
//!
//! **Program-first** (single-step, any registered op):
//! - `fibonacci` → single step `fibonacci` with inputs from op signature
//! - `quicksort` → single step `quicksort` with inputs from op signature
//! - etc.
//!
//! Concept labels are resolved to file patterns via the NL vocab's
//! `noun_patterns` table (e.g., `comic_issue_archive` → `*.cbz`, `*.cbr`).

use std::collections::HashMap;

use crate::nl::intent_ir::{IntentIR, IntentIRResult, UtteranceKind};
use crate::nl::vocab;
use crate::plan::{PlanDef, PlanInput, RawStep, StepArgs};

// ---------------------------------------------------------------------------
// Compilation result
// ---------------------------------------------------------------------------

/// Result of compiling an Intent IR to a PlanDef.
#[derive(Debug)]
pub enum CompileResult {
    /// Successfully compiled to a PlanDef.
    Ok(PlanDef),
    /// Compilation failed with an error message.
    Error(String),
    /// No intent to compile (empty input / gibberish).
    NoIntent,
    /// User approved the current plan.
    Approve,
    /// User rejected the current plan.
    Reject,
    /// User asked for an explanation.
    Explain { subject: String },
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compile an IntentIRResult to a PlanDef.
///
/// Uses the primary intent. Alternatives are preserved in DialogueState
/// by the caller.
pub fn compile_intent(result: &IntentIRResult) -> CompileResult {
    // Check utterance kind first — approve/reject/explain don't produce plans
    match &result.kind {
        UtteranceKind::Approve => return CompileResult::Approve,
        UtteranceKind::Reject => return CompileResult::Reject,
        UtteranceKind::Explain { subject } => return CompileResult::Explain { subject: subject.clone() },
        UtteranceKind::Command => {}
    }
    match &result.primary {
        None => CompileResult::NoIntent,
        Some(ir) => compile_ir(ir),
    }
}

// ---------------------------------------------------------------------------
// Declarative action→ops mapping
//
// Each filesystem action maps to a sequence of ops. The planner's
// lower_to_plan_def() converts these to RawSteps with params.
// ---------------------------------------------------------------------------

/// An action's op recipe: the sequence of ops to emit for a given action.
struct ActionRecipe {
    /// Ordered list of op names to emit.
    ops: &'static [&'static str],
}

/// Look up the op recipe for a filesystem action label.
/// Takes the IR for context-dependent recipes (e.g., compress file vs dir).
/// Returns None for unknown actions (they fall through to algorithm-op lookup).
fn action_recipe(action: &str, ir: &IntentIR) -> Option<ActionRecipe> {
    // Check if the input looks like a file path (for compress)
    let target_is_file = ir.inputs.first()
        .and_then(|i| i.selector.scope.as_ref())
        .map(|s| is_file_path(&s.dir))
        .unwrap_or(false);

    match action {
        "select" | "retrieve"      => Some(ActionRecipe { ops: &["walk_tree", "find_matching"] }),
        "traverse"                  => Some(ActionRecipe { ops: &["walk_tree"] }),
        "enumerate"                 => Some(ActionRecipe { ops: &["list_dir"] }),
        "compress" => {
            if target_is_file {
                Some(ActionRecipe { ops: &["gzip_compress"] })
            } else {
                Some(ActionRecipe { ops: &["walk_tree", "pack_archive"] })
            }
        }
        "decompress"                => Some(ActionRecipe { ops: &["extract_archive"] }),
        "search_text"               => Some(ActionRecipe { ops: &["walk_tree", "search_content"] }),
        "deduplicate"               => Some(ActionRecipe { ops: &["walk_tree", "find_duplicates"] }),
        "count"                     => Some(ActionRecipe { ops: &["walk_tree", "count_entries"] }),
        "order"                     => Some(ActionRecipe { ops: &["sort_by"] }),
        "read"                      => Some(ActionRecipe { ops: &["read_file"] }),
        "delete"                    => Some(ActionRecipe { ops: &["delete"] }),
        "copy"                      => Some(ActionRecipe { ops: &["copy"] }),
        "move"                      => Some(ActionRecipe { ops: &["move_entry"] }),
        "rename"                    => Some(ActionRecipe { ops: &["rename"] }),
        "compare"                   => Some(ActionRecipe { ops: &["diff"] }),
        "checksum"                  => Some(ActionRecipe { ops: &["checksum"] }),
        "download"                  => Some(ActionRecipe { ops: &["download"] }),
        "build"                     => Some(ActionRecipe { ops: &["build_project"] }),
        "test"                      => Some(ActionRecipe { ops: &["test_project"] }),
        "lint"                      => Some(ActionRecipe { ops: &["lint_project"] }),
        _ => None,
    }
}

/// Compile an IR step using the declarative action recipe.
/// Returns Some(Vec<RawStep>) if the action has a recipe, None otherwise.
fn try_action_recipe(
    action: &str,
    ir_step: &crate::nl::intent_ir::IRStep,
    ir: &IntentIR,
    prior_steps: usize,
) -> Option<Vec<RawStep>> {
    let recipe = action_recipe(action, ir)?;
    let mut steps = Vec::new();

    // Extract pattern from IR (concept resolution or explicit)
    let pattern = extract_pattern_from_ir(ir_step, ir);

    // If this is the first step and the action operates on sequences
    // but the input is a directory, prepend walk_tree.
    if prior_steps == 0 && action == "order" {
        steps.push(RawStep { op: "walk_tree".into(), args: StepArgs::None });
    }

    for &op in recipe.ops {


        let args = build_recipe_step_args(op, ir_step, &pattern);
        steps.push(RawStep { op: op.to_string(), args });
    }
    Some(steps)
}

/// Build step args for a recipe op based on IR params.
fn build_recipe_step_args(
    op: &str,
    ir_step: &crate::nl::intent_ir::IRStep,
    pattern: &Option<String>,
) -> StepArgs {
    match op {
        "find_matching" | "filter" => {
            if let Some(pat) = pattern {
                let mut m = HashMap::new();
                m.insert("pattern".to_string(), pat.clone());
                StepArgs::from_string_map(m)
            } else {
                StepArgs::None
            }
        }
        "sort_by" => {
            // The IR step for "order" has `by` and `direction` params.
            // We combine them into a single scalar mode string.
            let field = ir_step.params.get("by").map(|s| s.as_str()).unwrap_or("name");
            let direction = ir_step.params.get("direction").map(|s| s.as_str()).unwrap_or("ascending");

            let mode = match field {
                "modification_time" | "mtime" | "date" | "time" => {
                    if direction == "descending" { "mtime_desc" } else { "mtime" }
                }
                "size" => {
                    if direction == "descending" { "size_desc" } else { "size" }
                }
                "name" | "alphabetical" => {
                    if direction == "descending" { "name_desc" } else { "name" }
                }
                _ => "name",
            };

            StepArgs::Scalar(mode.to_string())
        }
        "search_content" => {
            if let Some(pat) = pattern {
                let mut m = HashMap::new();
                m.insert("pattern".to_string(), pat.clone());
                StepArgs::from_string_map(m)
            } else {
                StepArgs::None
            }
        }
        _ => StepArgs::None,
    }
}

/// Extract a file pattern from an IR step, resolving concepts to glob patterns.
fn extract_pattern_from_ir(
    ir_step: &crate::nl::intent_ir::IRStep,
    ir: &IntentIR,
) -> Option<String> {
    // 1. Explicit pattern in step params
    if let Some(pat) = ir_step.params.get("pattern") {
        return Some(pat.clone());
    }
    // 2. Concept from a select step's "where" clause
    if let Some(where_val) = ir_step.params.get("where") {
        if let Some(kind) = where_val.strip_prefix("kind: \"").and_then(|s| s.strip_suffix('"')) {
            let patterns = resolve_concept_to_patterns(kind);
            if !patterns.is_empty() {
                return Some(patterns.join(","));
            }
        }
    }
    // 3. Check other steps in the IR for concepts
    for step in &ir.steps {
        if step.action == "select" {
            if let Some(where_val) = step.params.get("where") {
                if let Some(kind) = where_val.strip_prefix("kind: \"").and_then(|s| s.strip_suffix('"')) {
                    let patterns = resolve_concept_to_patterns(kind);
                    if !patterns.is_empty() {
                        return Some(patterns.join(","));
                    }
                }
            }
            if let Some(pat) = step.params.get("pattern") {
                return Some(pat.clone());
            }
        }
    }
    None
}

/// Compile a single IntentIR to a PlanDef.
pub fn compile_ir(ir: &IntentIR) -> CompileResult {
    // ── Short-circuit: if the primary action is a registered algorithm op
    //    or a plan file, skip the filesystem IR entirely.
    //    Also check ALL step actions and select-step concepts. ──────────
    let primary_action = ir.steps.iter()
        .rfind(|s| s.action != "select" && s.action != "order")
        .map(|s| s.action.as_str());

    // Collect candidate action names: primary action + any select-step concepts
    let mut candidates: Vec<String> = Vec::new();
    if let Some(action) = primary_action {
        candidates.push(action.to_string());
    }
    // Check select steps for concepts that might be algorithm ops
    for step in &ir.steps {
        if step.action == "select" {
            if let Some(where_val) = step.params.get("where") {
                if let Some(kind) = where_val.strip_prefix("kind: \"").and_then(|s| s.strip_suffix('"')) {
                    candidates.push(kind.to_string());
                }
            }
        }
    }
    // Try each candidate
    // Also check ALL step actions (including select, order) as potential algorithm ops
    static FS_ACTIONS: &[&str] = &[
        "select", "order", "compress", "decompress", "enumerate", "traverse",
        "count", "read", "delete", "copy", "move", "rename", "deduplicate",
        "retrieve", "search_text",
    ];
    for step in &ir.steps {
        if !FS_ACTIONS.contains(&step.action.as_str()) {
            candidates.push(step.action.clone());
        }
    }
    let registry = crate::fs_types::build_full_registry();
    for candidate in &candidates {
        if let Some(plan) = try_load_plan_file(candidate) {
            return CompileResult::Ok(plan);
        }
        if let Some(entry) = registry.get_poly(candidate) {
            if entry.racket_body.is_some() {
                return compile_algorithm_op(candidate, entry, ir);
            }
        }
    }

    // Extract the target path from the IR's input selector
    let target_path = ir.inputs.first()
        .and_then(|i| i.selector.scope.as_ref())
        .map(|s| s.dir.clone())
        .unwrap_or_else(|| ".".to_string());

    // Try each IR step. For single-step IRs (the common case), this
    // produces a single plan. For multi-step IRs (select + order),
    // steps accumulate.
    let mut inputs: Vec<PlanInput> = Vec::new();
    let mut steps: Vec<RawStep> = Vec::new();
    let mut used_program_first = false;

    for ir_step in &ir.steps {
        let action = ir_step.action.as_str();

        // 1. Try declarative action recipe for known filesystem actions.
        if let Some(planned_steps) = try_action_recipe(action, ir_step, ir, steps.len()) {
            steps.extend(planned_steps);
            continue;
        }

        // 2. Program-first: check if the action label is a registered op.
        //    This handles algorithm atoms and any other registered op.
        let registry = crate::fs_types::build_full_registry();
        if let Some(op_entry) = registry.get_poly(action) {
            // Build inputs from the op's signature
            if !used_program_first {
                inputs = build_op_inputs(op_entry);
                used_program_first = true;
            }

            // Build step — if the op has input_names, pass them as params
            let args = build_op_step_args(op_entry, &ir.inputs);
            steps.push(RawStep {
                op: action.to_string(),
                args,
            });
            continue;
        }

        // 2b. Plan-file lookup: check if the action matches a plan YAML file.
        //     This handles multi-step DSL plans (factorial, zero_one_knapsack, etc.)
        if let Some(plan) = try_load_plan_file(action) {
            return CompileResult::Ok(plan);
        }

        // 3. Unknown action — error
        return CompileResult::Error(format!(
            "Unknown action '{}'. Try a command like 'find', 'sort', 'zip', 'extract', or an algorithm like 'fibonacci', 'quicksort'.",
            action
        ));
    }

    // If no steps were generated, that's an error
    if steps.is_empty() {
        return CompileResult::Error(
            "Could not determine what to do. Try something like 'compute fibonacci' or 'find comics in downloads'.".to_string()
        );
    }

    // Default inputs for filesystem ops
    if inputs.is_empty() {
        if is_file_path(&target_path) {
            inputs.push(PlanInput::bare("file"));
        } else {
            inputs.push(PlanInput::bare("path"));
        }
    }

    // Generate a plan name
    let name = if used_program_first && steps.len() == 1 {
        // For single-op programs, use the op name directly
        steps[0].op.clone().replace('_', "-")
    } else {
        generate_plan_name(&steps, &target_path)
    };

    // Bind path literals to inputs (the calling frame)
    let mut bindings = HashMap::new();
    if !used_program_first {
        let input_name = inputs.first()
            .map(|i| i.name.clone())
            .unwrap_or_else(|| "path".to_string());
        bindings.insert(input_name, target_path);
    }

    CompileResult::Ok(PlanDef {
        name,
        inputs,
        output: None,
        steps,
        bindings,
    })
}

// ---------------------------------------------------------------------------
// Program-first helpers
// ---------------------------------------------------------------------------

/// Build PlanInput list from a registered op's signature.
fn build_op_inputs(op: &crate::registry::PolyOpEntry) -> Vec<PlanInput> {
    if !op.input_names.is_empty() {
        // Use the op's declared input names with type hints from signature
        op.input_names.iter().zip(op.signature.inputs.iter())
            .map(|(name, type_expr)| {
                PlanInput::typed(name.clone(), type_expr.to_string())
            })
            .collect()
    } else {
        // Fallback: generate generic input names
        op.signature.inputs.iter().enumerate()
            .map(|(i, type_expr)| {
                let name = if op.signature.inputs.len() == 1 {
                    "input".to_string()
                } else {
                    format!("input_{}", i + 1)
                };
                PlanInput::typed(name, type_expr.to_string())
            })
            .collect()
    }
}

/// Build step args from op's input names and IR input values.
fn build_op_step_args(
    op: &crate::registry::PolyOpEntry,
    _ir_inputs: &[crate::nl::intent_ir::IRInput],
) -> StepArgs {
    // For algorithm atoms, the step args reference the plan inputs by $var.
    // The plan compiler will resolve these during compilation.
    if !op.input_names.is_empty() {
        let mut map: HashMap<String, String> = HashMap::new();
        for name in &op.input_names {
            map.insert(name.clone(), format!("${}", name));
        }
        StepArgs::from_string_map(map)
    } else {
        StepArgs::None
    }
}


// ---------------------------------------------------------------------------
// Plan-file lookup
// ---------------------------------------------------------------------------

/// Try to load a plan YAML file matching the given action name.
///
/// Searches `data/plans/algorithms/` for `<action>.yaml`.
/// Returns the parsed PlanDef if found, None otherwise. 
fn try_load_plan_file(action: &str) -> Option<PlanDef> {
    // Normalize: NL may produce hyphenated names, plan files use underscores
    let normalized = action.replace('-', "_");

    // 1. Try pipeline plans (data/plans/*.sexp)
    let pipeline_dir = std::path::Path::new("data/plans");
    if pipeline_dir.exists() {
        for ext in &["sexp", "yaml"] {
            let plan_path = pipeline_dir.join(format!("{}.{}", normalized, ext));
            if let Some(plan) = try_load_plan_at(&plan_path) {
                return Some(plan);
            }
        }
    }

    // 2. Try algorithm plans (data/plans/algorithms/<category>/*.sexp|*.yaml)
    let base = std::path::Path::new("data/plans/algorithms");
    if !base.exists() {
        return None;
    }

    // Walk category directories
    for cat_entry in std::fs::read_dir(base).ok()? {
        let cat_entry = cat_entry.ok()?;
        if !cat_entry.file_type().ok()?.is_dir() {
            continue;
        }

        // Try .sexp first, then .yaml
        for ext in &["sexp", "yaml"] {
            let plan_path = cat_entry.path().join(format!("{}.{}", normalized, ext));
            if let Some(plan) = try_load_plan_at(&plan_path) {
                return Some(plan);
            }
        }
    }

    None
}

/// Try to load and parse a plan from a specific file path.
fn try_load_plan_at(path: &std::path::Path) -> Option<PlanDef> {
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    let result = crate::sexpr::parse_sexpr_to_plan(&content).map_err(|e| e.to_string());
    result.ok()
}

/// Try to load the raw content of a plan file matching the given action name.
///
/// Returns the file content as a string if found, None otherwise.
pub fn try_load_plan_sexpr(action: &str) -> Option<String> {
    let normalized = action.replace('-', "_");

    // 1. Try pipeline plans (data/plans/*.sexp|*.yaml)
    let pipeline_dir = std::path::Path::new("data/plans");
    if pipeline_dir.exists() {
        for ext in &["sexp", "yaml"] {
            let plan_path = pipeline_dir.join(format!("{}.{}", normalized, ext));
            if plan_path.exists() {
                return std::fs::read_to_string(&plan_path).ok();
            }
        }
    }

    // 2. Try algorithm plans
    let base = std::path::Path::new("data/plans/algorithms");
    if !base.exists() { return None; }

    for cat_entry in std::fs::read_dir(base).ok()? {
        let cat_entry = cat_entry.ok()?;
        if !cat_entry.file_type().ok()?.is_dir() {
            continue;
        }

        // Try .sexp first, then .yaml
        for ext in &["sexp", "yaml"] {
            let plan_path = cat_entry.path().join(format!("{}.{}", normalized, ext));
            if plan_path.exists() {
                return std::fs::read_to_string(&plan_path).ok();
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Filesystem step compilation helpers
// ---------------------------------------------------------------------------

/// Compile a registered algorithm op into a single-step PlanDef.
///
/// This is the short-circuit path for algorithm atoms: the IR's filesystem
/// scaffolding (select, order) is ignored and we produce a clean plan with
/// the op's declared inputs and a single step.
pub(crate) fn compile_algorithm_op(
    action: &str,
    op: &crate::registry::PolyOpEntry,
    _ir: &IntentIR,
) -> CompileResult {
    let inputs = build_op_inputs(op);
    let args = build_op_step_args(op, &[]);
    let step = RawStep {
        op: action.to_string(),
        args,
    };

    let plan = PlanDef {
        name: action.to_string(),
        inputs,
        output: None,
        steps: vec![step],
        bindings: HashMap::new(),
    };

    CompileResult::Ok(plan)
}

/// Public helper: compile an algorithm op by name (no IR needed).
pub fn compile_algorithm_op_by_name(
    action: &str,
    op: &crate::registry::PolyOpEntry,
) -> PlanDef {
    let inputs = build_op_inputs(op);
    let args = build_op_step_args(op, &[]);
    let step = RawStep {
        op: action.to_string(),
        args,
    };
    PlanDef {
        name: action.to_string(),
        inputs,
        output: None,
        steps: vec![step],
        bindings: HashMap::new(),
    }
}


/// Check if a path looks like a file (has an extension).
fn is_file_path(path: &str) -> bool {
    if let Some(last) = path.rsplit('/').next() {
        last.contains('.') && !last.starts_with('.')
    } else {
        path.contains('.') && !path.starts_with('.')
    }
}

// ---------------------------------------------------------------------------
// Concept → pattern resolution
// ---------------------------------------------------------------------------

/// Resolve a concept label to file glob patterns using the NL vocab's
/// noun_patterns table.
fn resolve_concept_to_patterns(concept: &str) -> Vec<String> {
    let v = vocab::vocab();
    let lex = crate::nl::lexicon::lexicon();

    // Find all nouns that map to this concept
    for (word, info) in &lex.nouns {
        if info.concept == concept {
            if let Some(patterns) = v.noun_patterns.get(word.as_str()) {
                return patterns.clone();
            }
        }
    }

    // Fallback: try the concept label itself as a noun_pattern key
    if let Some(patterns) = v.noun_patterns.get(concept) {
        return patterns.clone();
    }

    Vec::new()
}

/// Generate a plan name from the steps and target path.
fn generate_plan_name(steps: &[RawStep], target_path: &str) -> String {
    if steps.is_empty() {
        return "unnamed_plan".to_string();
    }

    let ops: Vec<&str> = steps.iter().map(|s| s.op.as_str()).collect();

    let primary = if ops.contains(&"pack_archive") {
        "archive"
    } else if ops.contains(&"extract_archive") {
        "extract"
    } else if ops.contains(&"find_matching") {
        "find"
    } else if ops.contains(&"sort_by") {
        "sort"
    } else if ops.contains(&"list_dir") {
        "list"
    } else if ops.contains(&"walk_tree") {
        "walk"
    } else {
        ops.first().unwrap_or(&"plan")
    };

    let filter_info: Option<String> = steps.iter()
        .find(|s| s.op == "find_matching")
        .and_then(|s| match &s.args {
            StepArgs::Map(m) => m.get("pattern").and_then(|p| p.as_str().map(|s| s.to_string())),
            _ => None,
        });

    if let Some(pattern) = filter_info {
        let clean = pattern.replace("*.", "").replace(',', "-");
        if target_path != "." {
            let path_slug = slugify_path(target_path);
            format!("{}_{}_in_{}", primary, clean, path_slug)
        } else {
            format!("{}_{}", primary, clean)
        }
    } else if target_path != "." {
        let path_slug = slugify_path(target_path);
        format!("{}_in_{}", primary, path_slug)
    } else {
        primary.to_string()
    }
}

/// Turn a file path into a valid identifier slug.
fn slugify_path(path: &str) -> String {
    path.replace("~/", "")
        .replace('/', "_")
        .replace('-', "_")
        .replace('.', "_")
        .replace(' ', "_")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nl::earley;
    use crate::nl::grammar::build_command_grammar;
    use crate::nl::intent_ir;
    use crate::nl::lexicon::lexicon;

    fn compile_input(input: &str) -> CompileResult {
        let grammar = build_command_grammar();
        let lex = lexicon();
        let tokens: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();
        let parses = earley::parse(&grammar, &tokens, lex);
        let ir_result = intent_ir::parse_trees_to_intents(&parses);
        compile_intent(&ir_result)
    }

    fn expect_plan(input: &str) -> PlanDef {
        match compile_input(input) {
            CompileResult::Ok(plan) => plan,
            CompileResult::Error(e) => panic!("expected Ok, got Error: {}", e),
            CompileResult::NoIntent => panic!("expected Ok, got NoIntent"),
            other => panic!("expected Ok, got: {:?}", other),
        }
    }

    #[test]
    fn test_find_comics_in_downloads_newest_first() {
        let plan = expect_plan("find comics in my downloads folder newest first");

        let ops: Vec<&str> = plan.steps.iter().map(|s| s.op.as_str()).collect();
        assert!(ops.contains(&"walk_tree"), "should have walk_tree: {:?}", ops);
        assert!(ops.contains(&"find_matching"), "should have find_matching: {:?}", ops);
        assert!(ops.contains(&"sort_by"), "should have sort_by: {:?}", ops);

        let filter = plan.steps.iter().find(|s| s.op == "find_matching").unwrap();
        match &filter.args {
            StepArgs::Map(m) => {
                let pattern = m.get("pattern").and_then(|p| p.as_str()).expect("should have pattern");
                assert!(pattern.contains("cbz") || pattern.contains("cbr"),
                    "pattern should contain cbz/cbr: {}", pattern);
            }
            _ => panic!("find_matching should have Map args"),
        }

        let sort = plan.steps.iter().find(|s| s.op == "sort_by").unwrap();
        match &sort.args {
            StepArgs::Scalar(mode) => {
                assert!(mode.contains("mtime") && mode.contains("desc"),
                    "sort mode should be mtime_desc: {}", mode);
            }
            _ => panic!("sort_by should have Scalar args"),
        }
    }

    #[test]
    fn test_find_pdfs_in_documents() {
        let plan = expect_plan("find pdfs in documents");

        let ops: Vec<&str> = plan.steps.iter().map(|s| s.op.as_str()).collect();
        assert!(ops.contains(&"walk_tree"));
        assert!(ops.contains(&"find_matching"));

        let filter = plan.steps.iter().find(|s| s.op == "find_matching").unwrap();
        match &filter.args {
            StepArgs::Map(m) => {
                let pattern = m.get("pattern").and_then(|p| p.as_str()).expect("should have pattern");
                assert!(pattern.contains("pdf"), "pattern should contain pdf: {}", pattern);
            }
            _ => panic!("find_matching should have Map args"),
        }
    }

    #[test]
    fn test_zip_up_everything_in_downloads() {
        let plan = expect_plan("zip up everything in downloads");

        let ops: Vec<&str> = plan.steps.iter().map(|s| s.op.as_str()).collect();
        assert!(ops.contains(&"walk_tree"), "should have walk_tree: {:?}", ops);
        assert!(ops.contains(&"pack_archive"), "should have pack_archive: {:?}", ops);
    }

    #[test]
    fn test_extract_archive() {
        let plan = expect_plan("extract ~/comic.cbz");

        let ops: Vec<&str> = plan.steps.iter().map(|s| s.op.as_str()).collect();
        assert!(ops.contains(&"extract_archive"), "should have extract_archive: {:?}", ops);
    }

    #[test]
    fn test_list_bare_verb() {
        let plan = expect_plan("list");

        let ops: Vec<&str> = plan.steps.iter().map(|s| s.op.as_str()).collect();
        assert!(ops.contains(&"list_dir"), "should have list_dir: {:?}", ops);
    }

    #[test]
    fn test_sort_files_newest_first() {
        let plan = expect_plan("sort files newest first");

        let ops: Vec<&str> = plan.steps.iter().map(|s| s.op.as_str()).collect();
        assert!(ops.contains(&"sort_by"), "should have sort_by: {:?}", ops);
    }

    #[test]
    fn test_empty_input_no_intent() {
        match compile_input("") {
            CompileResult::NoIntent => {}
            other => panic!("expected NoIntent, got: {:?}", other),
        }
    }

    #[test]
    fn test_gibberish_no_intent() {
        match compile_input("asdfghjkl qwerty") {
            CompileResult::NoIntent => {}
            other => panic!("expected NoIntent, got: {:?}", other),
        }
    }

    #[test]
    fn test_concept_resolution_comics() {
        let patterns = resolve_concept_to_patterns("comic_issue_archive");
        assert!(!patterns.is_empty(), "should resolve comic_issue_archive to patterns");
        assert!(patterns.iter().any(|p| p.contains("cbz")),
            "should include *.cbz: {:?}", patterns);
    }

    #[test]
    fn test_concept_resolution_pdf() {
        let patterns = resolve_concept_to_patterns("pdf_document");
        assert!(!patterns.is_empty(), "should resolve pdf_document to patterns");
        assert!(patterns.iter().any(|p| p.contains("pdf")),
            "should include *.pdf: {:?}", patterns);
    }

    #[test]
    fn test_concept_resolution_unknown() {
        let patterns = resolve_concept_to_patterns("nonexistent_concept");
        assert!(patterns.is_empty(), "unknown concept should resolve to empty: {:?}", patterns);
    }

    #[test]
    fn test_plan_has_path_input() {
        let plan = expect_plan("find comics in downloads");
        assert!(!plan.inputs.is_empty());
        assert_eq!(plan.inputs[0].name, "path");
    }

    #[test]
    fn test_plan_name_generated() {
        let plan = expect_plan("find comics in downloads");
        assert!(!plan.name.is_empty(), "plan name should not be empty");
        assert!(plan.name.contains("find") || plan.name.contains("cbz"),
            "plan name should be descriptive: {}", plan.name);
    }

    #[test]
    fn test_no_steps_produces_walk_tree() {
        let plan = expect_plan("find files");
        let ops: Vec<&str> = plan.steps.iter().map(|s| s.op.as_str()).collect();
        assert!(ops.contains(&"walk_tree"), "should have walk_tree: {:?}", ops);
    }

    #[test]
    fn test_bindings_populated_with_path() {
        let plan = expect_plan("find comics in downloads");
        assert!(!plan.bindings.is_empty(), "bindings should not be empty for path literal");
        let input_name = &plan.inputs[0].name;
        assert!(plan.bindings.contains_key(input_name),
            "bindings should contain input '{}': {:?}", input_name, plan.bindings);
        let bound_value = &plan.bindings[input_name];
        assert!(bound_value.contains("ownload"),
            "bound value should contain 'ownload': {}", bound_value);
    }

    #[test]
    fn test_bindings_default_path_when_no_path() {
        let plan = expect_plan("find files");
        // When no path is given, "." should be bound as default
        let input_name = &plan.inputs[0].name;
        assert_eq!(plan.bindings.get(input_name).map(|s| s.as_str()), Some("."),
            "should bind default path '.': {:?}", plan.bindings);
    }

    #[test]
    fn test_bindings_shown_in_yaml() {
        use crate::nl::dialogue::plan_to_sexpr;
        let plan = expect_plan("find comics in downloads");
        let yaml = plan_to_sexpr(&plan);
        assert!(yaml.contains("ownload"),
            "YAML should contain bound path value: {}", yaml);
    }

    #[test]
    fn test_bindings_with_home_dir() {
        let plan = expect_plan("list files in home");
        if !plan.bindings.is_empty() {
            let input_name = &plan.inputs[0].name;
            let bound = &plan.bindings[input_name];
            assert!(!bound.is_empty(), "bound path should not be empty");
        }
    }

    // -- Program-first tests --

    #[test]
    fn test_program_first_registered_op() {
        // Directly test the program-first path with a known algorithm op
        use crate::nl::intent_ir::{IntentIR, IRStep, IRInput, IRSelector};

        let ir = IntentIR {
            output: "Number".to_string(),
            inputs: vec![IRInput {
                name: "n".to_string(),
                type_expr: "Number".to_string(),
                selector: IRSelector { scope: None },
            }],
            steps: vec![IRStep {
                action: "fibonacci".to_string(),
                input_refs: vec![],
                output_ref: "result".to_string(),
                params: HashMap::new(),
            }],
            constraints: vec![],
            acceptance: vec![],
            score: 1.0,
        };

        let result = compile_ir(&ir);
        match result {
            CompileResult::Ok(plan) => {
                assert_eq!(plan.steps.len(), 1);
                assert_eq!(plan.steps[0].op, "fibonacci");
                assert_eq!(plan.name, "fibonacci");
                // Should have typed input from op signature
                assert!(!plan.inputs.is_empty());
                assert_eq!(plan.inputs[0].name, "n");
            }
            other => panic!("expected Ok, got: {:?}", other),
        }
    }

    #[test]
    fn test_program_first_quicksort() {
        use crate::nl::intent_ir::{IntentIR, IRStep, IRInput, IRSelector};

        let ir = IntentIR {
            output: "List(Number)".to_string(),
            inputs: vec![IRInput {
                name: "lst".to_string(),
                type_expr: "List(Number)".to_string(),
                selector: IRSelector { scope: None },
            }],
            steps: vec![IRStep {
                action: "quicksort".to_string(),
                input_refs: vec![],
                output_ref: "result".to_string(),
                params: HashMap::new(),
            }],
            constraints: vec![],
            acceptance: vec![],
            score: 1.0,
        };

        let result = compile_ir(&ir);
        match result {
            CompileResult::Ok(plan) => {
                assert_eq!(plan.steps.len(), 1);
                assert_eq!(plan.steps[0].op, "quicksort");
                // Should have typed input
                assert!(!plan.inputs.is_empty());
            }
            other => panic!("expected Ok, got: {:?}", other),
        }
    }

    #[test]
    fn test_program_first_unknown_action_errors() {
        use crate::nl::intent_ir::{IntentIR, IRStep, IRInput, IRSelector};

        let ir = IntentIR {
            output: "Any".to_string(),
            inputs: vec![IRInput {
                name: "x".to_string(),
                type_expr: "Any".to_string(),
                selector: IRSelector { scope: None },
            }],
            steps: vec![IRStep {
                action: "nonexistent_op_xyz_123".to_string(),
                input_refs: vec![],
                output_ref: "result".to_string(),
                params: HashMap::new(),
            }],
            constraints: vec![],
            acceptance: vec![],
            score: 1.0,
        };

        let result = compile_ir(&ir);
        match result {
            CompileResult::Error(msg) => {
                assert!(msg.contains("nonexistent_op_xyz_123"),
                    "error should mention the unknown action: {}", msg);
            }
            other => panic!("expected Error, got: {:?}", other),
        }
    }

    #[test]
    fn test_program_first_plan_name_is_op_name() {
        use crate::nl::intent_ir::{IntentIR, IRStep, IRInput, IRSelector};

        let ir = IntentIR {
            output: "Number".to_string(),
            inputs: vec![IRInput {
                name: "n".to_string(),
                type_expr: "Number".to_string(),
                selector: IRSelector { scope: None },
            }],
            steps: vec![IRStep {
                action: "digital_root".to_string(),
                input_refs: vec![],
                output_ref: "result".to_string(),
                params: HashMap::new(),
            }],
            constraints: vec![],
            acceptance: vec![],
            score: 1.0,
        };

        let result = compile_ir(&ir);
        match result {
            CompileResult::Ok(plan) => {
                assert_eq!(plan.name, "digital_root");
            }
            other => panic!("expected Ok, got: {:?}", other),
        }
    }

    #[test]
    fn test_filesystem_actions_still_work() {
        // Verify that filesystem-specific actions are not broken
        // by the program-first path
        let plan = expect_plan("find pdfs in documents");
        let ops: Vec<&str> = plan.steps.iter().map(|s| s.op.as_str()).collect();
        assert!(ops.contains(&"walk_tree"));
        assert!(ops.contains(&"find_matching"));
    }

    // -- Recipe system tests --

    /// Helper: build an IR with a single action step and compile it.
    fn compile_action(action: &str, params: HashMap<String, String>) -> CompileResult {
        use crate::nl::intent_ir::{IntentIR, IRStep, IRInput, IRSelector};

        let ir = IntentIR {
            output: "Any".to_string(),
            inputs: vec![IRInput {
                name: "path".to_string(),
                type_expr: "Dir".to_string(),
                selector: IRSelector { scope: None },
            }],
            steps: vec![IRStep {
                action: action.to_string(),
                input_refs: vec![],
                output_ref: "result".to_string(),
                params,
            }],
            constraints: vec![],
            acceptance: vec![],
            score: 1.0,
        };
        compile_ir(&ir)
    }

    fn expect_ops(action: &str, params: HashMap<String, String>) -> Vec<String> {
        match compile_action(action, params) {
            CompileResult::Ok(plan) => plan.steps.iter().map(|s| s.op.clone()).collect(),
            other => panic!("expected Ok for '{}', got: {:?}", action, other),
        }
    }

    #[test]
    fn test_recipe_select() {
        let ops = expect_ops("select", HashMap::new());
        assert_eq!(ops, vec!["walk_tree", "find_matching"]);
    }

    #[test]
    fn test_recipe_retrieve() {
        let ops = expect_ops("retrieve", HashMap::new());
        assert_eq!(ops, vec!["walk_tree", "find_matching"]);
    }

    #[test]
    fn test_recipe_traverse() {
        let ops = expect_ops("traverse", HashMap::new());
        assert_eq!(ops, vec!["walk_tree"]);
    }

    #[test]
    fn test_recipe_enumerate() {
        let ops = expect_ops("enumerate", HashMap::new());
        assert_eq!(ops, vec!["list_dir"]);
    }

    #[test]
    fn test_recipe_decompress() {
        let ops = expect_ops("decompress", HashMap::new());
        assert_eq!(ops, vec!["extract_archive"]);
    }

    #[test]
    fn test_recipe_compress_dir() {
        // No file path in scope → directory compress
        let ops = expect_ops("compress", HashMap::new());
        assert_eq!(ops, vec!["walk_tree", "pack_archive"]);
    }

    #[test]
    fn test_recipe_compress_file() {
        use crate::nl::intent_ir::{IntentIR, IRStep, IRInput, IRSelector, IRScope};

        let ir = IntentIR {
            output: "Any".to_string(),
            inputs: vec![IRInput {
                name: "file".to_string(),
                type_expr: "File".to_string(),
                selector: IRSelector {
                    scope: Some(IRScope { dir: "~/report.txt".to_string(), recursive: false }),
                },
            }],
            steps: vec![IRStep {
                action: "compress".to_string(),
                input_refs: vec![],
                output_ref: "result".to_string(),
                params: HashMap::new(),
            }],
            constraints: vec![],
            acceptance: vec![],
            score: 1.0,
        };
        match compile_ir(&ir) {
            CompileResult::Ok(plan) => {
                let ops: Vec<&str> = plan.steps.iter().map(|s| s.op.as_str()).collect();
                assert_eq!(ops, vec!["gzip_compress"]);
            }
            other => panic!("expected Ok, got: {:?}", other),
        }
    }

    #[test]
    fn test_recipe_search_text() {
        let ops = expect_ops("search_text", HashMap::new());
        assert_eq!(ops, vec!["walk_tree", "search_content"]);
    }

    #[test]
    fn test_recipe_deduplicate() {
        let ops = expect_ops("deduplicate", HashMap::new());
        assert_eq!(ops, vec!["walk_tree", "find_duplicates"]);
    }

    #[test]
    fn test_recipe_count() {
        let ops = expect_ops("count", HashMap::new());
        assert_eq!(ops, vec!["walk_tree", "count_entries"]);
    }

    #[test]
    fn test_recipe_order_prepends_walk_tree() {
        // When order is the first step, walk_tree is prepended
        let ops = expect_ops("order", HashMap::new());
        assert_eq!(ops, vec!["walk_tree", "sort_by"]);
    }

    #[test]
    fn test_recipe_order_sort_mode_mtime_desc() {
        let mut params = HashMap::new();
        params.insert("by".to_string(), "modification_time".to_string());
        params.insert("direction".to_string(), "descending".to_string());
        match compile_action("order", params) {
            CompileResult::Ok(plan) => {
                let sort = plan.steps.iter().find(|s| s.op == "sort_by").unwrap();
                match &sort.args {
                    StepArgs::Scalar(mode) => assert_eq!(mode, "mtime_desc"),
                    other => panic!("expected Scalar, got: {:?}", other),
                }
            }
            other => panic!("expected Ok, got: {:?}", other),
        }
    }

    #[test]
    fn test_recipe_order_sort_mode_size() {
        let mut params = HashMap::new();
        params.insert("by".to_string(), "size".to_string());
        params.insert("direction".to_string(), "ascending".to_string());
        match compile_action("order", params) {
            CompileResult::Ok(plan) => {
                let sort = plan.steps.iter().find(|s| s.op == "sort_by").unwrap();
                match &sort.args {
                    StepArgs::Scalar(mode) => assert_eq!(mode, "size"),
                    other => panic!("expected Scalar, got: {:?}", other),
                }
            }
            other => panic!("expected Ok, got: {:?}", other),
        }
    }

    #[test]
    fn test_recipe_single_ops() {
        // Test all single-op recipes
        assert_eq!(expect_ops("read", HashMap::new()), vec!["read_file"]);
        assert_eq!(expect_ops("delete", HashMap::new()), vec!["delete"]);
        assert_eq!(expect_ops("copy", HashMap::new()), vec!["copy"]);
        assert_eq!(expect_ops("move", HashMap::new()), vec!["move_entry"]);
        assert_eq!(expect_ops("rename", HashMap::new()), vec!["rename"]);
        assert_eq!(expect_ops("compare", HashMap::new()), vec!["diff"]);
        assert_eq!(expect_ops("checksum", HashMap::new()), vec!["checksum"]);
        assert_eq!(expect_ops("download", HashMap::new()), vec!["download"]);
    }

    #[test]
    fn test_recipe_unknown_action_falls_through() {
        // Unknown action should NOT match a recipe — falls through to error
        match compile_action("nonexistent_action_xyz", HashMap::new()) {
            CompileResult::Error(_) => {} // expected
            other => panic!("expected Error for unknown action, got: {:?}", other),
        }
    }

    #[test]
    fn test_recipe_select_with_pattern() {
        let mut params = HashMap::new();
        params.insert("pattern".to_string(), "*.pdf".to_string());
        match compile_action("select", params) {
            CompileResult::Ok(plan) => {
                let filter = plan.steps.iter().find(|s| s.op == "find_matching").unwrap();
                match &filter.args {
                    StepArgs::Map(m) => {
                        let pat = m.get("pattern").and_then(|v| v.as_str()).unwrap();
                        assert_eq!(pat, "*.pdf");
                    }
                    other => panic!("expected Map, got: {:?}", other),
                }
            }
            other => panic!("expected Ok, got: {:?}", other),
        }
    }
}
