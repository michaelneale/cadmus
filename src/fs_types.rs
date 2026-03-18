use crate::registry::{OperationRegistry, load_ops_pack_str, load_ops_pack_str_into};

// ---------------------------------------------------------------------------
// Filesystem type vocabulary
// ---------------------------------------------------------------------------
//
// Primitives:  Path, Bytes, Name, Text, Line, Pattern, Metadata
// Constructors: File(content), Dir(entry), Archive(content, format),
//               Seq(elem), Entry(key, val), Match(pattern, val), Tree(node),
//               Option(inner)
// Format tags:  Zip, Cbz, Tar, TarGz

/// The embedded filesystem ops pack YAML.
///
/// Phase 3 migration: the on-disk `data/fs.ops.yaml` file has been deleted.
/// These ops are now registered as compatibility aliases — the actual
/// execution is routed through the shell-callable subsumption map in
/// `src/type_lowering.rs`. The YAML is kept embedded so that old op names
/// (list_dir, walk_tree, etc.) remain resolvable by the plan compiler
/// and NL layer.
const FS_OPS_YAML: &str = include_str!("../data/packs/ops/fs.ops.yaml");

/// The embedded power tools ops pack YAML (compatibility aliases).
const POWER_TOOLS_OPS_YAML: &str = include_str!("../data/packs/ops/power_tools.ops.yaml");

// ---------------------------------------------------------------------------
// Registry builders
// ---------------------------------------------------------------------------

/// Build and return an OperationRegistry populated with the filesystem
/// type vocabulary from the ops pack YAML.
///
/// Uses the embedded copy of fs.ops.yaml (the on-disk file has been deleted
/// as part of the Phase 3 shell migration). These ops serve as compatibility
/// aliases — execution is routed through the subsumption map.
pub fn build_fs_registry() -> OperationRegistry {
    load_ops_pack_str(FS_OPS_YAML)
        .expect("embedded fs.ops.yaml should always parse")
}

/// Build a full registry with both filesystem and power tools ops.
///
/// Used by the plan system and other contexts that need the complete
/// set of operations. Loads embedded compatibility aliases from the old
/// YAML packs, then runs inference to discover shell-callable forms.
/// The embedded racket ops pack YAML, used as fallback when the file
/// is not found on disk.
const RACKET_OPS_YAML: &str = include_str!("../data/packs/ops/racket.ops.yaml");
const RACKET_FACTS_YAML: &str = include_str!("../data/packs/facts/racket.facts.yaml");

/// The embedded macOS CLI facts YAML, used as fallback when the file is not found on disk.
const MACOS_CLI_FACTS_YAML: &str = include_str!("../data/packs/facts/macos_cli.facts.yaml");

/// The embedded algorithm ops pack YAML (opaque algorithm atoms).
const ALGORITHM_OPS_YAML: &str = include_str!("../data/packs/ops/algorithm.ops.yaml");
const TEXT_PROCESSING_OPS_YAML: &str = include_str!("../data/packs/ops/text_processing.ops.yaml");
const STATISTICS_OPS_YAML: &str = include_str!("../data/packs/ops/statistics.ops.yaml");
const MACOS_TASKS_OPS_YAML: &str = include_str!("../data/packs/ops/macos_tasks.ops.yaml");
const CODE_EDITING_OPS_YAML: &str = include_str!("../data/packs/ops/code_editing.ops.yaml");
const WEB_OPS_YAML: &str = include_str!("../data/packs/ops/web.ops.yaml");

pub fn build_full_registry() -> OperationRegistry {
    // Start with embedded fs_ops (compatibility aliases)
    let mut reg = load_ops_pack_str(FS_OPS_YAML)
        .expect("embedded fs.ops.yaml should always parse");

    // Merge embedded power tools ops (compatibility aliases)
    let _ = load_ops_pack_str_into(POWER_TOOLS_OPS_YAML, &mut reg);

    // Merge racket ops
    let _ = load_ops_pack_str_into(
        &std::fs::read_to_string("data/packs/ops/racket.ops.yaml")
            .unwrap_or_else(|_| RACKET_OPS_YAML.to_string()),
        &mut reg,
    );

    // Merge algorithm ops (opaque atoms with racket_body)
    let _ = load_ops_pack_str_into(
        &std::fs::read_to_string("data/packs/ops/algorithm.ops.yaml")
            .unwrap_or_else(|_| ALGORITHM_OPS_YAML.to_string()),
        &mut reg,
    );

    // Merge text processing ops
    let _ = load_ops_pack_str_into(
        &std::fs::read_to_string("data/packs/ops/text_processing.ops.yaml")
            .unwrap_or_else(|_| TEXT_PROCESSING_OPS_YAML.to_string()),
        &mut reg,
    );

    // Merge statistics ops
    let _ = load_ops_pack_str_into(
        &std::fs::read_to_string("data/packs/ops/statistics.ops.yaml")
            .unwrap_or_else(|_| STATISTICS_OPS_YAML.to_string()),
        &mut reg,
    );

    // Merge macOS tasks ops (complex multi-command operations with racket_body)
    let _ = load_ops_pack_str_into(
        &std::fs::read_to_string("data/packs/ops/macos_tasks.ops.yaml")
            .unwrap_or_else(|_| MACOS_TASKS_OPS_YAML.to_string()),
        &mut reg,
    );

    // Merge code editing ops (search, navigate, edit, build)
    let _ = load_ops_pack_str_into(
        &std::fs::read_to_string("data/packs/ops/code_editing.ops.yaml")
            .unwrap_or_else(|_| CODE_EDITING_OPS_YAML.to_string()),
        &mut reg,
    );

    // Merge web ops (HTTP server operations with racket_body)
    let _ = load_ops_pack_str_into(
        &std::fs::read_to_string("data/packs/ops/web.ops.yaml")
            .unwrap_or_else(|_| WEB_OPS_YAML.to_string()),
        &mut reg,
    );


    // Run inference to discover and promote ops from the fact pack
    // (e.g., subtract, multiply, divide are discovered from racket.facts.yaml)
    if let Ok(facts) = crate::racket_strategy::load_racket_facts_from_str(
        &std::fs::read_to_string("data/packs/facts/racket.facts.yaml")
            .unwrap_or_else(|_| RACKET_FACTS_YAML.to_string()),
    ) {
        crate::racket_strategy::promote_inferred_ops(&mut reg, &facts);

        // Phase 4: Shell submode discovery from macOS CLI facts
        // Reads submode_ properties from the CLI fact pack and creates
        // variadic form ops for each tool's flag variants.
        let cli_yaml = std::fs::read_to_string("data/packs/facts/macos_cli.facts.yaml")
            .unwrap_or_else(|_| MACOS_CLI_FACTS_YAML.to_string());
        if let Ok(cli_pack) = serde_yaml::from_str::<crate::fact_pack::FactPack>(&cli_yaml) {
            let cli_facts = crate::fact_pack::FactPackIndex::build(cli_pack);
            crate::racket_strategy::discover_shell_submodes(
                &mut reg, &facts, &cli_facts,
            );
        }
    }

    reg
}

// ---------------------------------------------------------------------------
// Op description lookup (cached)
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::OnceLock;

static OP_DESCRIPTIONS: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Get the description map for all registered ops (cached singleton).
///
/// Built from the full registry (fs_ops + power_tools + coding + comparison).
/// Returns a map of op_name → description string.
fn op_descriptions() -> &'static HashMap<String, String> {
    OP_DESCRIPTIONS.get_or_init(|| {
        let reg = build_full_registry();
        let mut map = HashMap::new();
        for name in reg.poly_op_names() {
            if let Some(op) = reg.get_poly(name) {
                map.insert(name.to_string(), op.description.clone());
            }
        }
        map
    })
}

/// Get the description for a specific op, or None if not found.
pub fn get_op_description(op_name: &str) -> Option<&'static str> {
    op_descriptions().get(op_name).map(|s| s.as_str())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_expr::TypeExpr;

    #[test]
    fn test_all_ops_registered() {
        let reg = build_fs_registry();
        let names = reg.poly_op_names();
        assert!(names.len() >= 49, "expected at least 49 ops, got {}", names.len());

        let expected = vec![
            // Original 15 + walk_tree_hierarchy + flatten_tree
            "list_dir", "read_file", "write_file", "stat", "walk_tree",
            "walk_tree_hierarchy", "flatten_tree",
            "filter", "sort_by", "extract_archive", "pack_archive",
            "concat_seq", "rename", "move_entry", "search_content",
            "find_matching", "map_entries",
            // Phase 1: file lifecycle
            "copy", "delete", "create_dir", "create_link", "set_permissions", "set_owner",
            // Phase 2: content transforms
            "replace", "head", "tail", "unique", "count", "diff", "checksum",
            // Phase 3: metadata accessors
            "get_size", "get_mtime", "get_permissions", "get_file_type",
            // Phase 4: macOS-specific
            "spotlight_search", "get_xattr", "set_xattr", "remove_xattr",
            "remove_quarantine", "open_file", "open_with", "reveal",
            "clipboard_copy", "clipboard_paste", "read_plist", "write_plist",
            // Phase 5: network
            "download", "upload", "sync",
        ];
        for name in &expected {
            assert!(
                reg.get_poly(name).is_some(),
                "missing op: {}",
                name
            );
        }
    }

    #[test]
    fn test_ops_lookupable_by_output() {
        let reg = build_fs_registry();

        // list_dir produces Seq(Entry(Name, Bytes))
        let target = TypeExpr::seq(TypeExpr::entry(
            TypeExpr::prim("Name"),
            TypeExpr::prim("Bytes"),
        ));
        let matches = reg.ops_for_output_expr(&target);
        let names: Vec<&str> = matches.iter().map(|m| m.op.name.as_str()).collect();
        assert!(names.contains(&"list_dir"), "list_dir should match Seq(Entry(Name, Bytes)), got: {:?}", names);
    }

    #[test]
    fn test_extract_archive_unifies_with_zip_and_cbz() {
        let reg = build_fs_registry();

        // Zip archive
        let zip_target = TypeExpr::seq(TypeExpr::entry(
            TypeExpr::prim("Name"),
            TypeExpr::prim("Bytes"),
        ));
        let zip_matches = reg.ops_for_output_expr(&zip_target);
        let zip_names: Vec<&str> = zip_matches.iter().map(|m| m.op.name.as_str()).collect();
        assert!(zip_names.contains(&"extract_archive"), "extract_archive should match for Zip content");

        // CBZ archive (content is File(Image))
        let cbz_target = TypeExpr::seq(TypeExpr::entry(
            TypeExpr::prim("Name"),
            TypeExpr::file(TypeExpr::prim("Image")),
        ));
        let cbz_matches = reg.ops_for_output_expr(&cbz_target);
        let cbz_names: Vec<&str> = cbz_matches.iter().map(|m| m.op.name.as_str()).collect();
        assert!(cbz_names.contains(&"extract_archive"), "extract_archive should match for CBZ content");

        // Verify the input types are correctly specialized
        for m in &cbz_matches {
            if m.op.name == "extract_archive" {
                // Input should be File(Archive(File(Image), <var>))
                assert_eq!(m.concrete_inputs.len(), 1);
                let input = &m.concrete_inputs[0];
                match input {
                    TypeExpr::Constructor(name, args) if name == "File" => {
                        match &args[0] {
                            TypeExpr::Constructor(name2, args2) if name2 == "Archive" => {
                                assert_eq!(args2[0], TypeExpr::file(TypeExpr::prim("Image")));
                            }
                            other => panic!("expected Archive, got: {}", other),
                        }
                    }
                    other => panic!("expected File(Archive(...)), got: {}", other),
                }
            }
        }
    }

    #[test]
    fn test_concat_seq_associative_not_commutative() {
        let reg = build_fs_registry();
        let op = reg.get_poly("concat_seq").unwrap();
        assert!(op.properties.associative, "concat_seq should be associative");
        assert!(!op.properties.commutative, "concat_seq should NOT be commutative");
    }

    #[test]
    fn test_filter_is_idempotent() {
        let reg = build_fs_registry();
        let op = reg.get_poly("filter").unwrap();
        assert!(op.properties.idempotent, "filter should be idempotent");
    }

    #[test]
    fn test_sort_by_is_idempotent() {
        let reg = build_fs_registry();
        let op = reg.get_poly("sort_by").unwrap();
        assert!(op.properties.idempotent, "sort_by should be idempotent");
    }

    #[test]
    fn test_stat_is_idempotent() {
        let reg = build_fs_registry();
        let op = reg.get_poly("stat").unwrap();
        assert!(op.properties.idempotent, "stat should be idempotent");
    }

    #[test]
    fn test_pack_archive_signature() {
        let reg = build_fs_registry();
        let op = reg.get_poly("pack_archive").unwrap();
        assert_eq!(op.signature.inputs.len(), 2, "pack_archive needs 2 inputs");
        assert_eq!(op.signature.type_params.len(), 2, "pack_archive has 2 type params");
    }

    #[test]
    fn test_search_content_monomorphic() {
        let reg = build_fs_registry();
        let op = reg.get_poly("search_content").unwrap();
        assert!(op.signature.type_params.is_empty(), "search_content is monomorphic");
        assert_eq!(op.signature.inputs.len(), 2);
    }

    #[test]
    fn test_find_matching_idempotent() {
        let reg = build_fs_registry();
        let op = reg.get_poly("find_matching").unwrap();
        assert!(op.properties.idempotent, "find_matching should be idempotent");
    }

    #[test]
    fn test_concat_seq_has_identity() {
        let reg = build_fs_registry();
        let op = reg.get_poly("concat_seq").unwrap();
        assert_eq!(op.properties.identity, Some("[]".to_string()));
    }

    #[test]
    fn test_build_fs_registry_includes_power_tools() {
        let reg = build_full_registry();
        // fs_ops
        assert!(reg.get_poly("list_dir").is_some(), "missing fs op list_dir");
        assert!(reg.get_poly("read_file").is_some(), "missing fs op read_file");
        // power_tools ops
        assert!(reg.get_poly("git_log").is_some(), "missing power_tools op git_log");
        assert!(reg.get_poly("jq_query").is_some(), "missing power_tools op jq_query");
        assert!(reg.get_poly("tmux_new_session").is_some(), "missing power_tools op tmux_new_session");
        assert!(reg.get_poly("ps_list").is_some(), "missing power_tools op ps_list");
        // Total should be fs_ops + power_tools
        assert!(reg.poly_op_names().len() >= 100, "expected at least 100 ops in merged registry, got {}", reg.poly_op_names().len());
    }
}
