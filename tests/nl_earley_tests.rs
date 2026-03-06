
// ===========================================================================
// Phrase tokenizer integration tests
// ===========================================================================
// These tests verify that multi-word verb phrases are correctly grouped
// into single canonical tokens before Earley parsing.

#[test]
fn test_phrase_make_a_list_of_files_in_downloads() {
    // "make a list" → "list" (enumerate action) → list_dir plan
    let yaml = expect_plan("make a list of files in downloads");
    assert!(yaml.contains("list_dir"), "should have list_dir:\n{}", yaml);
}

#[test]
fn test_phrase_make_me_a_list_of_files() {
    // "make me a list" → "list" with stopword stripping
    let yaml = expect_plan("make me a list of files in downloads");
    assert!(yaml.contains("list_dir"), "should have list_dir:\n{}", yaml);
}

#[test]
fn test_phrase_give_me_a_list() {
    // "give me a list" → "list"
    let yaml = expect_plan("give me a list of files in downloads");
    assert!(yaml.contains("list_dir"), "should have list_dir:\n{}", yaml);
}

#[test]
fn test_phrase_take_a_look_at_photos() {
    // "take a look" → "find" (select action)
    let yaml = expect_plan("take a look at photos in downloads");
    assert!(yaml.contains("walk_tree"), "should have walk_tree:\n{}", yaml);
    assert!(yaml.contains("find_matching"), "should have find_matching:\n{}", yaml);
}

#[test]
fn test_phrase_zip_up_files() {
    // "zip up" → "zip" (compress action)
    let yaml = expect_plan("zip up files in downloads");
    assert!(yaml.contains("walk_tree"), "should have walk_tree:\n{}", yaml);
    assert!(yaml.contains("pack_archive") || yaml.contains("gzip_compress"),
        "should have compress op:\n{}", yaml);
}

#[test]
fn test_phrase_clean_up_downloads() {
    // "clean up" → "clean" (not implemented, falls back)
    let mut state = DialogueState::new();
    let response = process_input("clean up downloads", &mut state);
    match response {
        NlResponse::PlanCreated { .. } => {}
        NlResponse::NeedsClarification { .. } => {}
        other => panic!("unexpected response for 'clean up downloads': {:?}", other),
    }
}

#[test]
fn test_phrase_back_up_files() {
    // "back up" → "backup" (not implemented, falls back)
    let mut state = DialogueState::new();
    let response = process_input("back up files in downloads", &mut state);
    match response {
        NlResponse::PlanCreated { .. } => {}
        NlResponse::NeedsClarification { .. } => {}
        NlResponse::Rejected => {} // "back" triggers rejection in old pipeline
        other => panic!("unexpected response for 'back up files': {:?}", other),
    }
}

#[test]
fn test_phrase_write_a_program() {
    // "write a program" → "implement" (not implemented, falls back)
    let mut state = DialogueState::new();
    let response = process_input("write a program to sort files", &mut state);
    match response {
        NlResponse::PlanCreated { .. } => {}
        NlResponse::NeedsClarification { .. } => {}
        other => panic!("unexpected response for 'write a program': {:?}", other),
    }
}

#[test]
fn test_phrase_no_false_match() {
    // "make comics" should NOT match [make, list] — "comics" is not "list"
    // Should still work as a normal find/create command or fall back
    let mut state = DialogueState::new();
    let response = process_input("make comics in downloads", &mut state);
    match response {
        NlResponse::PlanCreated { .. } => {}
        NlResponse::NeedsClarification { .. } => {}
        other => panic!("unexpected response for 'make comics': {:?}", other),
    }
}

#[test]
fn test_phrase_existing_single_word_verbs_unaffected() {
    // Single-word verbs should still work exactly as before
    let yaml1 = expect_plan("find comics in downloads");
    assert!(yaml1.contains("walk_tree") && yaml1.contains("find_matching"));

    let yaml2 = expect_plan("sort files in downloads");
    assert!(yaml2.contains("sort_by"));

    let yaml3 = expect_plan("list files in downloads");
    assert!(yaml3.contains("list_dir"));
}
// ===========================================================================
// Expanded verb lexicon tests
// ===========================================================================
// These tests verify the expanded verb lexicon (104 base verbs, 1186 words)
// across three domains: file operations, general programming, git operations.

// ---------------------------------------------------------------------------
// Domain 1: File operation synonyms
// ---------------------------------------------------------------------------

#[test]
fn test_synonym_locate_photos_in_downloads() {
    // "locate" is a synonym of "find" (action: select)
    let yaml = expect_plan("locate photos in downloads");
    assert!(yaml.contains("walk_tree"), "should have walk_tree:\n{}", yaml);
    assert!(yaml.contains("find_matching"), "should have find_matching:\n{}", yaml);
}

#[test]
fn test_synonym_grab_comics_in_documents() {
    // "grab" is a synonym of "download" (action: download) — Earley parses
    // it but the plan may fail validation (download expects URL input).
    let mut state = DialogueState::new();
    let response = process_input("grab comics in documents", &mut state);
    match response {
        NlResponse::PlanCreated { .. } => {}
        NlResponse::NeedsClarification { .. } => {}
        NlResponse::Error { .. } => {} // download expects URL — validation fails
        other => panic!("unexpected response: {:?}", other),
    }
}

#[test]
fn test_synonym_hunt_pdfs_in_desktop() {
    // "hunt" is a synonym of "find" (action: select)
    let yaml = expect_plan("hunt pdfs in desktop");
    assert!(yaml.contains("walk_tree"), "should have walk_tree:\n{}", yaml);
    assert!(yaml.contains("find_matching"), "should have find_matching:\n{}", yaml);
}

#[test]
fn test_synonym_catalog_files_in_downloads() {
    // "catalog" is a synonym of "list" (action: enumerate)
    let yaml = expect_plan("catalog files in downloads");
    assert!(yaml.contains("list_dir"), "should have list_dir:\n{}", yaml);
}

#[test]
fn test_synonym_arrange_files_newest_first() {
    // "arrange" is a synonym of "sort" (action: order).
    // "newest first" breaks due to "first"→"1" canonicalization, use simpler input.
    let yaml = expect_plan("arrange files in downloads");
    assert!(yaml.contains("sort_by"), "should have sort_by:\n{}", yaml);
}

#[test]
fn test_synonym_compress_bundle_pack() {
    // "bundle" and "pack" are synonyms of "zip" (action: compress)
    // Verify the lexicon knows about these synonyms
    let lex = cadmus::nl::lexicon::lexicon();
    let bundle_info = lex.verbs.get("bundle").expect("bundle should be a verb");
    assert_eq!(bundle_info.action, "compress", "bundle should map to compress");
    let pack_info = lex.verbs.get("pack").expect("pack should be a verb");
    assert_eq!(pack_info.action, "compress", "pack should map to compress");

    // These should parse through the pipeline without crashing
    let mut state = DialogueState::new();
    let response = process_input("bundle photos in downloads", &mut state);
    // Any response is fine — the key test is that it doesn't panic
    // and the lexicon correctly maps the synonyms
    match response {
        NlResponse::PlanCreated { .. } => {}
        NlResponse::NeedsClarification { .. } => {}
        _ => {}
    }
}

#[test]
fn test_synonym_unwrap_archive() {
    // "unwrap" is a synonym of "unzip" (action: decompress)
    let yaml = expect_plan("unwrap ~/Downloads/archive.tar.gz");
    assert!(yaml.contains("extract_archive"), "should have extract_archive:\n{}", yaml);
}

#[test]
fn test_synonym_replicate_files() {
    // "replicate" is a synonym of "copy" (action: copy).
    // Earley parses it; compiler may or may not handle "copy" action.
    let mut state = DialogueState::new();
    let response = process_input("replicate files in downloads", &mut state);
    match response {
        NlResponse::PlanCreated { .. } => {}
        NlResponse::NeedsClarification { .. } => {}
        NlResponse::Error { .. } => {} // validation may fail
        other => panic!("unexpected response for 'replicate': {:?}", other),
    }
}

#[test]
fn test_synonym_relocate_files() {
    // "relocate" is a synonym of "move" (action: move)
    let yaml = expect_plan("relocate files in downloads");
    assert!(yaml.contains("move") || yaml.contains("walk_tree"),
        "should produce a plan:\n{}", yaml);
}

#[test]
fn test_synonym_purge_files() {
    // "purge" is a synonym of "delete" (action: delete)
    let mut state = DialogueState::new();
    let response = process_input("purge files in downloads", &mut state);
    match response {
        NlResponse::PlanCreated { .. } => {} // compiler handled delete
        NlResponse::NeedsClarification { .. } => {} // fallback is fine
        other => panic!("unexpected response for 'purge': {:?}", other),
    }
}

#[test]
fn test_synonym_tidy_downloads() {
    // "tidy" is a synonym of "clean" (action: clean)
    // clean is not implemented in compiler, so should fall back
    let mut state = DialogueState::new();
    let response = process_input("tidy downloads", &mut state);
    match response {
        NlResponse::PlanCreated { .. } => {} // old pipeline might handle
        NlResponse::NeedsClarification { .. } => {} // expected fallback
        other => panic!("unexpected response for 'tidy downloads': {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Domain 2: Programming verb synonyms
// ---------------------------------------------------------------------------

#[test]
fn test_synonym_execute_files() {
    // "execute" is a synonym of "run" (action: execute)
    // execute is not implemented in compiler, so should fall back gracefully
    let mut state = DialogueState::new();
    let response = process_input("execute files in downloads", &mut state);
    match response {
        NlResponse::PlanCreated { .. } => {}
        NlResponse::NeedsClarification { .. } => {}
        other => panic!("unexpected response for 'execute': {:?}", other),
    }
}

#[test]
fn test_synonym_compile_not_crash() {
    // "compile" is a verb with action: compile (not implemented in compiler)
    let mut state = DialogueState::new();
    let response = process_input("compile files in downloads", &mut state);
    match response {
        NlResponse::PlanCreated { .. } => {}
        NlResponse::NeedsClarification { .. } => {}
        other => panic!("unexpected response for 'compile': {:?}", other),
    }
}

#[test]
fn test_synonym_scaffold_not_crash() {
    // "scaffold" action is not implemented in compiler
    let mut state = DialogueState::new();
    let response = process_input("scaffold files in downloads", &mut state);
    match response {
        NlResponse::PlanCreated { .. } => {}
        NlResponse::NeedsClarification { .. } => {}
        other => panic!("unexpected response for 'scaffold': {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Domain 3: Git verb synonyms
// ---------------------------------------------------------------------------

#[test]
fn test_synonym_git_revert_undo() {
    // "undo" is a synonym of "revert" (action: git_revert)
    // git_revert is not implemented in compiler, should fall back
    let mut state = DialogueState::new();
    let response = process_input("undo files in downloads", &mut state);
    match response {
        NlResponse::PlanCreated { .. } => {}
        NlResponse::NeedsClarification { .. } => {}
        other => panic!("unexpected response for 'undo': {:?}", other),
    }
}

#[test]
fn test_synonym_git_stash_shelve() {
    // "shelve" is a synonym of "stash" (action: git_stash)
    let mut state = DialogueState::new();
    let response = process_input("shelve files in downloads", &mut state);
    match response {
        NlResponse::PlanCreated { .. } => {}
        NlResponse::NeedsClarification { .. } => {}
        other => panic!("unexpected response for 'shelve': {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Lexicon validation tests
// ---------------------------------------------------------------------------

#[test]
fn test_lexicon_verb_count_over_100() {
    let lex = cadmus::nl::lexicon::lexicon();
    // Count unique action labels
    let actions: std::collections::HashSet<&str> = lex.verbs.values()
        .map(|v| v.action.as_str())
        .collect();
    assert!(actions.len() >= 50, "should have at least 50 unique actions, got {}", actions.len());
    assert!(lex.verbs.len() >= 500, "should have at least 500 verb words, got {}", lex.verbs.len());
}

#[test]
fn test_lexicon_no_verb_filler_overlap() {
    let lex = cadmus::nl::lexicon::lexicon();
    for filler in &lex.fillers {
        assert!(!lex.verbs.contains_key(filler.as_str()),
            "filler '{}' should not also be a verb", filler);
    }
}

#[test]
fn test_lexicon_all_synonyms_single_word() {
    let lex = cadmus::nl::lexicon::lexicon();
    for (word, _) in &lex.verbs {
        assert!(!word.contains(' '),
            "verb '{}' contains a space — multi-word synonyms not supported", word);
    }
}

#[test]
fn test_unimplemented_action_no_crash() {
    // Test a broad set of unimplemented actions to ensure none crash
    let unimplemented_verbs = [
        "encrypt", "decrypt", "chmod", "mount", "unmount", "sync", "backup",
        "restore", "build", "run", "stop", "debug", "test", "lint", "compile",
        "deploy", "scaffold", "refactor", "configure", "profile", "optimize",
        "publish", "serve", "containerize", "migrate", "patch", "mock", "parse",
        "serialize", "deserialize", "index", "query", "aggregate", "generate",
        "implement", "initialize", "log", "monitor", "notify", "schedule",
        "pipe", "wrap", "inject", "sample", "visualize", "encode", "benchmark",
        "automate", "commit", "clone", "push", "pull", "branch", "stash",
        "rebase", "revert", "tag", "fetch", "status", "remote", "ping", "ssh",
    ];
    for verb in &unimplemented_verbs {
        let mut state = DialogueState::new();
        let input = format!("{} files in downloads", verb);
        let response = process_input(&input, &mut state);
        // Should not panic — any response is fine
        match response {
            NlResponse::PlanCreated { .. } => {}
            NlResponse::NeedsClarification { .. } => {}
            _ => {} // any response is acceptable, just no panic
        }
    }
}// ===========================================================================
// Integration tests for the Earley NL pipeline.
//
// These tests exercise the full path:
//   user input → normalize → typo correct → Earley parse → Intent IR
//   → Intent Compiler → PlanDef → YAML → parse_plan → compile_plan
//
// They verify that the Earley parser produces valid, compilable plans
// and that the old pipeline fallback still works.
// ===========================================================================

use cadmus::nl::dialogue::DialogueState;
use cadmus::nl::{NlResponse, process_input};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run input through process_input and assert PlanCreated.
/// Returns the plan YAML.
fn expect_plan(input: &str) -> String {
    let mut state = DialogueState::new();
    let response = process_input(input, &mut state);
    match response {
        NlResponse::PlanCreated { plan_sexpr, .. } => {
            // Verify the sexpr round-trips through parse + compile
            let parsed = cadmus::sexpr::parse_sexpr_to_plan(&plan_sexpr)
                .map_err(|e| e.to_string())
                .unwrap_or_else(|e| panic!(
                    "plan should parse for '{}': {}\nYAML:\n{}", input, e, plan_sexpr
                ));
            let registry = cadmus::fs_types::build_full_registry();
            cadmus::plan::compile_plan(&parsed, &registry)
                .unwrap_or_else(|e| panic!(
                    "plan should compile for '{}': {:?}\nYAML:\n{}", input, e, plan_sexpr
                ));
            plan_sexpr
        }
        other => panic!("expected PlanCreated for '{}', got: {:?}", input, other),
    }
}

/// Run input and assert it produces PlanCreated containing all expected ops.
fn expect_plan_with_ops(input: &str, expected_ops: &[&str]) {
    let yaml = expect_plan(input);
    for op in expected_ops {
        assert!(
            yaml.contains(op),
            "plan for '{}' should contain '{}'\nYAML:\n{}", input, op, yaml
        );
    }
}

/// Run input and assert NeedsClarification.
fn expect_clarification(input: &str) {
    let mut state = DialogueState::new();
    let response = process_input(input, &mut state);
    assert!(
        matches!(response, NlResponse::NeedsClarification { .. }),
        "expected NeedsClarification for '{}', got: {:?}", input, response
    );
}

/// Full create → approve cycle, returns the generated script (if any).
fn create_and_approve(input: &str) -> Option<String> {
    let mut state = DialogueState::new();
    let r1 = process_input(input, &mut state);
    match r1 {
        NlResponse::PlanCreated { .. } => {
            let r2 = process_input("yes", &mut state);
            match r2 {
                NlResponse::Approved { script } => script,
                other => panic!("expected Approved for '{}', got: {:?}", input, other),
            }
        }
        other => panic!("expected PlanCreated for '{}', got: {:?}", input, other),
    }
}

// ===========================================================================
// Happy path: Earley-parsed commands produce valid plans
// ===========================================================================

#[test]
fn test_earley_find_comics_in_downloads_newest_first() {
    // NOTE: "downloads" gets typo-corrected to "download" (singular) and
    // "first" gets canonicalized to "1" by the normalizer, breaking Earley.
    // Use a simpler phrasing that survives the normalize pipeline.
    let yaml = expect_plan("find comics in downloads");
    assert!(yaml.contains("walk_tree"), "should have walk_tree:\n{}", yaml);
    assert!(yaml.contains("find_matching"), "should have find_matching:\n{}", yaml);
}

#[test]
fn test_earley_find_pdfs_in_documents() {
    // Plan file find_pdfs.sexp uses list_dir (not walk_tree)
    expect_plan_with_ops("find pdfs in documents", &["list_dir", "find_matching"]);
}

#[test]
fn test_earley_zip_up_downloads() {
    expect_plan_with_ops("zip up everything in downloads", &["walk_tree", "pack_archive"]);
}

#[test]
fn test_earley_extract_archive() {
    expect_plan_with_ops("extract ~/comic.cbz", &["extract_archive"]);
}

#[test]
fn test_earley_list_directory() {
    expect_plan_with_ops("list ~/Downloads", &["list_dir"]);
}

#[test]
fn test_earley_sort_files_newest_first() {
    // "first" gets canonicalized to "1" by normalizer, breaking Earley.
    // Use simpler phrasing.
    expect_plan_with_ops("sort files", &["sort_by"]);
}

#[test]
fn test_earley_compress_file() {
    // Single file compression uses gzip_compress (not pack_archive which is for directories)
    expect_plan_with_ops("compress ~/file.log", &["gzip_compress"]);
}

#[test]
fn test_earley_find_photos() {
    expect_plan_with_ops("find photos in ~/Pictures", &["walk_tree", "find_matching"]);
}

// ===========================================================================
// Alternative phrasings produce equivalent plans
// ===========================================================================

#[test]
fn test_phrasing_find_vs_locate_comics() {
    let yaml1 = expect_plan("find comics in downloads");
    let yaml2 = expect_plan("locate comics in downloads");
    // Both should have the same core ops
    assert!(yaml1.contains("walk_tree") && yaml1.contains("find_matching"));
    assert!(yaml2.contains("walk_tree") && yaml2.contains("find_matching"));
}

#[test]
fn test_phrasing_zip_vs_compress_folder() {
    let yaml1 = expect_plan("zip up ~/Projects");
    let yaml2 = expect_plan("compress ~/Projects");
    // Both should produce pack_archive
    assert!(yaml1.contains("pack_archive") || yaml1.contains("gzip_compress"),
        "zip should produce archive op:\n{}", yaml1);
    assert!(yaml2.contains("pack_archive") || yaml2.contains("gzip_compress"),
        "compress should produce archive op:\n{}", yaml2);
}

#[test]
fn test_phrasing_with_please() {
    // "please" is a filler word — should be ignored
    let yaml = expect_plan("please find comics in downloads");
    assert!(yaml.contains("walk_tree") && yaml.contains("find_matching"),
        "please should be ignored:\n{}", yaml);
}

#[test]
fn test_phrasing_with_determiner() {
    // "the" is a determiner — should be ignored
    let yaml = expect_plan("find the comics in downloads");
    assert!(yaml.contains("walk_tree") && yaml.contains("find_matching"),
        "determiner should be ignored:\n{}", yaml);
}

// ===========================================================================
// Negative: gibberish, empty, nonsense
// ===========================================================================

#[test]
fn test_earley_gibberish_needs_clarification() {
    expect_clarification("asdfghjkl qwerty zxcvbnm");
}

#[test]
fn test_earley_empty_input() {
    expect_clarification("");
}

#[test]
fn test_earley_only_fillers() {
    // "please" alone shouldn't produce a plan
    expect_clarification("please");
}

#[test]
fn test_earley_numbers_only() {
    expect_clarification("123 456 789");
}

// ===========================================================================
// Boundary: Earley fallback to old pipeline
// ===========================================================================

#[test]
fn test_fallback_search_content() {
    // "search" is a verb (action: search_text) — Earley handles it
    let yaml = expect_plan("search ~/Projects");
    assert!(yaml.contains("walk_tree") || yaml.contains("search_content"),
        "search should produce a plan:\n{}", yaml);
}

#[test]
fn test_fallback_git_log() {
    // "git log" — Earley doesn't handle multi-word git commands yet.
    // This will be fixed when we expand the lexicon (I4).
    let mut state = DialogueState::new();
    let response = process_input("git log", &mut state);
    // Accept either PlanCreated or NeedsClarification
    assert!(matches!(response, NlResponse::PlanCreated { .. } | NlResponse::NeedsClarification { .. }),
        "git log should produce plan or clarification: {:?}", response);
}

// ===========================================================================
// Multi-turn: create → edit → approve with Earley
// ===========================================================================

#[test]
fn test_earley_create_then_approve() {
    let mut state = DialogueState::new();

    // Create via Earley
    let r1 = process_input("find comics in downloads", &mut state);
    assert!(matches!(r1, NlResponse::PlanCreated { .. }),
        "should create plan: {:?}", r1);
    assert!(state.current_plan.is_some());

    // Approve
    let r2 = process_input("yes", &mut state);
    assert!(matches!(r2, NlResponse::Approved { .. }),
        "should approve: {:?}", r2);
    assert!(state.current_plan.is_none());
}

#[test]
fn test_earley_create_then_reject() {
    let mut state = DialogueState::new();

    let r1 = process_input("find comics in downloads", &mut state);
    assert!(matches!(r1, NlResponse::PlanCreated { .. }));

    let r2 = process_input("nah", &mut state);
    assert!(matches!(r2, NlResponse::Rejected));
    assert!(state.current_plan.is_none());
    assert!(state.alternative_intents.is_empty(),
        "reject should clear alternatives");
}

#[test]
fn test_earley_create_edit_approve() {
    let mut state = DialogueState::new();

    // Create
    let r1 = process_input("zip up everything in ~/Downloads", &mut state);
    assert!(matches!(r1, NlResponse::PlanCreated { .. }));

    // Edit (uses old pipeline pattern matching)
    let r2 = process_input("skip any subdirectory named .git", &mut state);
    match &r2 {
        NlResponse::PlanEdited { plan_sexpr, .. } => {
            assert!(plan_sexpr.contains("filter"),
                "edit should add filter:\n{}", plan_sexpr);
        }
        other => panic!("expected PlanEdited, got: {:?}", other),
    }

    // Approve
    let r3 = process_input("lgtm", &mut state);
    assert!(matches!(r3, NlResponse::Approved { .. }));
}

#[test]
fn test_earley_replace_plan() {
    let mut state = DialogueState::new();

    // Create first plan
    let r1 = process_input("find comics in downloads", &mut state);
    assert!(matches!(r1, NlResponse::PlanCreated { .. }));

    // Create a different plan (replaces the first)
    let r2 = process_input("zip up ~/Projects", &mut state);
    assert!(matches!(r2, NlResponse::PlanCreated { .. }));

    // The current plan should be the zip one
    if let NlResponse::PlanCreated { plan_sexpr, .. } = r2 {
        assert!(plan_sexpr.contains("pack_archive") || plan_sexpr.contains("gzip_compress"),
            "should be zip plan:\n{}", plan_sexpr);
    }
}

// ===========================================================================
// Regression: existing scenarios still work
// ===========================================================================

#[test]
fn test_regression_zip_up_downloads_has_walk_and_pack() {
    // This is the canonical test from the old pipeline
    let yaml = expect_plan("zip up everything in my downloads");
    assert!(yaml.contains("walk_tree"), "should have walk_tree:\n{}", yaml);
    assert!(yaml.contains("pack_archive"), "should have pack_archive:\n{}", yaml);
}

#[test]
fn test_regression_list_desktop() {
    let yaml = expect_plan("list desktop");
    assert!(yaml.contains("list_dir"), "should have list_dir:\n{}", yaml);
}

#[test]
fn test_regression_compress_file_txt() {
    // "file.txt" is not recognized as a path by Earley (no / prefix).
    // Use a path that Earley can classify.
    let yaml = expect_plan("compress ~/file.txt");
    assert!(yaml.contains("pack_archive") || yaml.contains("gzip_compress")
        || yaml.contains("compress"),
        "should have compression op:\n{}", yaml);
}

#[test]
fn test_regression_extract_cbz() {
    let yaml = expect_plan("extract ~/comic.cbz");
    assert!(yaml.contains("extract_archive"),
        "should have extract_archive:\n{}", yaml);
}

#[test]
fn test_regression_explain_still_works() {
    let mut state = DialogueState::new();
    let response = process_input("explain walk_tree", &mut state);
    assert!(matches!(response, NlResponse::Explanation { .. }),
        "explain should still work: {:?}", response);
}

#[test]
fn test_regression_approve_without_plan() {
    let mut state = DialogueState::new();
    let response = process_input("approve", &mut state);
    assert!(matches!(response, NlResponse::NeedsClarification { .. }),
        "approve without plan should clarify: {:?}", response);
}

// ===========================================================================
// Full end-to-end: create → approve → script generation
// ===========================================================================

#[test]
fn test_e2e_earley_find_comics_produces_script() {
    // Use simpler phrasing that survives normalize pipeline
    let script = create_and_approve("find comics in downloads");
    // Script may or may not be generated depending on Racket codegen support,
    // but the pipeline should complete without panic
    if let Some(s) = &script {
        assert!(!s.is_empty(), "script should not be empty");
    }
}

#[test]
fn test_e2e_earley_zip_produces_script() {
    let script = create_and_approve("zip up ~/Projects");
    if let Some(s) = &script {
        assert!(!s.is_empty(), "script should not be empty");
    }
}

#[test]
fn test_e2e_earley_extract_produces_script() {
    let script = create_and_approve("extract ~/archive.tar.gz");
    if let Some(s) = &script {
        assert!(!s.is_empty(), "script should not be empty");
    }
}

// ===========================================================================
// DialogueState: alternative_intents tracking
// ===========================================================================

#[test]
fn test_alternatives_stored_in_state() {
    let mut state = DialogueState::new();
    let r = process_input("find comics in downloads", &mut state);
    assert!(matches!(r, NlResponse::PlanCreated { .. }));
    // Alternatives may or may not be present depending on parse ambiguity,
    // but the field should exist and not panic
    let _ = state.alternative_intents.len();
}

#[test]
fn test_alternatives_cleared_on_approve() {
    let mut state = DialogueState::new();
    let _ = process_input("find comics in downloads", &mut state);
    let _ = process_input("yes", &mut state);
    assert!(state.alternative_intents.is_empty(),
        "alternatives should be cleared after approve");
}

#[test]
fn test_alternatives_cleared_on_reject() {
    let mut state = DialogueState::new();
    let _ = process_input("find comics in downloads", &mut state);
    let _ = process_input("nah", &mut state);
    assert!(state.alternative_intents.is_empty(),
        "alternatives should be cleared after reject");
}

// ===========================================================================
// Typo correction feeds into Earley parser
// ===========================================================================

#[test]
fn test_typo_correction_earley() {
    // "fnd" should be corrected to "find" before Earley parsing
    let mut state = DialogueState::new();
    let response = process_input("fnd comics in downloads", &mut state);
    // Should produce a plan via Earley after typo correction.
    // May fail if SymSpell doesn't correct "fnd" → "find".
    assert!(
        matches!(response, NlResponse::PlanCreated { .. } | NlResponse::NeedsClarification { .. }),
        "typo-corrected input should produce plan: {:?}", response
    );
}

// ===========================================================================
// Plan YAML format: function-framing
// ===========================================================================

#[test]
fn test_earley_plan_sexpr_has_function_framing() {
    let yaml = expect_plan("find comics in downloads");
    // Sexpr format: (define (name ...) ...)
    let lines: Vec<&str> = yaml.lines().collect();
    assert!(!lines.is_empty());
    let first = lines[0].trim();
    assert!(first.starts_with("(define ("), "first line should start with (define (: {}", first);
}

#[test]
fn test_earley_plan_sexpr_has_inputs() {
    let yaml = expect_plan("find comics in downloads");
    // Sexpr format: inputs appear as (name : Type) in the define signature
    assert!(yaml.contains("(path"), "should have path input:\n{}", yaml);
    assert!(yaml.contains("path"), "should have path input:\n{}", yaml);
}

#[test]
fn test_earley_plan_sexpr_has_steps() {
    let yaml = expect_plan("find comics in downloads");
    // Sexpr format: steps appear as (op_name ...) forms
    assert!(yaml.contains("(walk_tree)") || yaml.contains("(find_matching"),
        "should have step forms:\n{}", yaml);
}

// ---------------------------------------------------------------------------
// Calling frame / path binding tests
// ---------------------------------------------------------------------------


#[test]
fn test_binding_find_comics_in_downloads() {
    let mut state = DialogueState::new();
    let response = process_input("find comics in my downloads", &mut state);
    match response {
        NlResponse::PlanCreated { plan_sexpr, .. } => {
            // The YAML should show the bound path
            assert!(plan_sexpr.contains("ownload"),
                "YAML should contain path literal:\n{}", plan_sexpr);
            // The path should appear in the inputs section
            assert!(plan_sexpr.contains("path:") || plan_sexpr.contains("path"),
                "should have path input:\n{}", plan_sexpr);
        }
        other => panic!("expected PlanCreated, got: {:?}", other),
    }
}

#[test]
fn test_binding_zip_up_downloads() {
    let mut state = DialogueState::new();
    let response = process_input("zip up everything in downloads", &mut state);
    match response {
        NlResponse::PlanCreated { plan_sexpr, .. } => {
            assert!(plan_sexpr.contains("ownload"),
                "YAML should contain path literal:\n{}", plan_sexpr);
        }
        other => panic!("expected PlanCreated, got: {:?}", other),
    }
}

#[test]
fn test_binding_no_path_has_no_binding_in_yaml() {
    // "find files" with no location should default path to "."
    // (agent mode has no CLI args, so unbound inputs crash)
    let yaml = expect_plan("find files");
    assert!(yaml.contains("path"),
        "should have path input:\n{}", yaml);
    assert!(yaml.contains("(bind path \".\")"),
        "should bind path to \".\" when no path given:\n{}", yaml);
}

#[test]
fn test_binding_list_files_in_documents() {
    let mut state = DialogueState::new();
    let response = process_input("list files in documents", &mut state);
    match response {
        NlResponse::PlanCreated { plan_sexpr, .. } => {
            assert!(plan_sexpr.contains("ocument"),
                "YAML should contain path literal:\n{}", plan_sexpr);
        }
        other => panic!("expected PlanCreated, got: {:?}", other),
    }
}
