//! Lexicon loader for the Earley parser.
//!
//! Loads word categories from `data/nl/nl_lexicon.yaml`. Each category
//! (verb, noun, preposition, etc.) maps surface words to canonical labels.
//! The lexicon drives the grammar's terminal symbols — adding a new word
//! is a YAML edit, not a code change.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::nl::earley::TokenClassifier;

// ---------------------------------------------------------------------------
// Embedded fallback
// ---------------------------------------------------------------------------

const EMBEDDED_LEXICON: &str = include_str!("../../data/nl/nl_lexicon.yaml");

// ---------------------------------------------------------------------------
// YAML schema
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct LexiconYaml {
    verbs: Vec<VerbEntry>,
    nouns: Vec<NounEntry>,
    #[serde(default)]
    approvals: Option<ApprovalRejectionLex>,
    #[serde(default)]
    rejections: Option<ApprovalRejectionLex>,
    #[serde(default)]
    explain_triggers: Vec<String>,
    prepositions: Vec<String>,
    possessives: Vec<String>,
    determiners: Vec<String>,
    path_nouns: Vec<PathNounEntry>,
    orderings: Vec<OrderingEntry>,
    quantifiers: Vec<String>,
    conjunctions: Vec<String>,
    fillers: Vec<String>,
    #[serde(default)]
    phrase_groups: Vec<PhraseGroupEntry>,
    #[serde(default)]
    concept_ops: Vec<ConceptOpEntry>,
}

#[derive(Debug, Deserialize)]
struct ConceptOpEntry {
    action: String,
    concept: String,
    op: String,
}
#[derive(Debug, Deserialize)]
struct ApprovalRejectionLex {
    single: Vec<String>,
    #[serde(default)]
    multi: Vec<String>,
}

/// A phrase group: multi-word verb phrase → canonical single token.
#[derive(Debug, Clone)]
pub struct PhraseGroup {
    /// Content-word skeleton (lowercased, 2+ words).
    pub skeleton: Vec<String>,
    /// Canonical verb token to emit.
    pub canonical: String,
}

#[derive(Debug, Deserialize)]
struct VerbEntry {
    word: String,
    action: String,
    #[serde(default)]
    synonyms: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PhraseGroupEntry {
    /// Content-word skeleton (2+ words, in order).
    skeleton: Vec<String>,
    /// Canonical single verb token to replace the phrase with.
    canonical: String,
}

#[derive(Debug, Deserialize)]
struct NounEntry {
    word: String,
    concept: String,
    #[serde(default)]
    synonyms: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PathNounEntry {
    word: String,
    path: String,
    #[serde(default)]
    synonyms: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OrderingEntry {
    phrase: String,
    field: String,
    direction: String,
    #[serde(default)]
    synonyms: Vec<String>,
}

// ---------------------------------------------------------------------------
// Runtime lexicon
// ---------------------------------------------------------------------------

/// A verb entry with its abstract action label.
#[derive(Debug, Clone)]
pub struct VerbInfo {
    /// The canonical surface word.
    pub word: String,
    /// The abstract action label (e.g., "select", "compress", "order").
    pub action: String,
}

/// A noun entry with its concept label.
#[derive(Debug, Clone)]
pub struct NounInfo {
    /// The canonical surface word.
    pub word: String,
    /// The concept label (e.g., "comic_issue_archive", "pdf_document").
    pub concept: String,
}

/// A path noun entry mapping a directory name to a filesystem path.
#[derive(Debug, Clone)]
pub struct PathNounInfo {
    /// The canonical surface word.
    pub word: String,
    /// The filesystem path (e.g., "~/Downloads").
    pub path: String,
}

/// An ordering entry mapping a phrase to sort parameters.
#[derive(Debug, Clone)]
pub struct OrderingInfo {
    /// The canonical phrase.
    pub phrase: String,
    /// The field to sort by (e.g., "modification_time", "size", "name").
    pub field: String,
    /// The sort direction ("ascending" or "descending").
    pub direction: String,
}

/// The loaded lexicon, indexed for fast lookup.
#[derive(Debug)]
pub struct Lexicon {
    /// Token → list of grammar categories it belongs to.
    /// E.g., "find" → ["verb"], "in" → ["preposition"], "my" → ["possessive", "determiner"].
    pub categories: HashMap<String, Vec<String>>,

    /// Verb lookup: surface word → VerbInfo.
    pub verbs: HashMap<String, VerbInfo>,

    /// Noun lookup: surface word → NounInfo.
    pub nouns: HashMap<String, NounInfo>,

    /// Path noun lookup: surface word → PathNounInfo.
    pub path_nouns: HashMap<String, PathNounInfo>,

    /// Ordering lookup: surface phrase/word → OrderingInfo.
    pub orderings: HashMap<String, OrderingInfo>,

    /// Set of filler words.
    pub fillers: std::collections::HashSet<String>,

    /// Set of prepositions.
    pub prepositions: std::collections::HashSet<String>,

    /// Set of possessives.
    pub possessives: std::collections::HashSet<String>,

    /// Set of determiners.
    pub determiners: std::collections::HashSet<String>,

    /// Set of quantifiers.
    pub quantifiers: std::collections::HashSet<String>,

    /// Set of conjunctions.
    pub conjunctions: std::collections::HashSet<String>,

    /// Phrase groups for multi-word verb phrase canonicalization.
    pub phrase_groups: Vec<PhraseGroup>,

    /// Single-word approval tokens.
    pub approval_singles: std::collections::HashSet<String>,
    /// Multi-word approval phrases (space-separated).
    pub approval_multis: Vec<String>,
    /// Single-word rejection tokens.
    pub rejection_singles: std::collections::HashSet<String>,
    /// Multi-word rejection phrases (space-separated).
    pub rejection_multis: Vec<String>,
    /// Explain trigger words.
    pub explain_triggers: std::collections::HashSet<String>,

    /// Concept ops: (action, concept) → op_name.
    /// Maps verb_action × noun_concept to a concrete op or plan name.
    pub concept_ops: HashMap<(String, String), String>,
}

impl Lexicon {
    /// Check if the tokens represent an approval.
    ///
    /// Matches single-word approvals, multi-word approval phrases,
    /// and compound approvals like "ok lgtm", "sure why not".
    pub fn is_approve(&self, tokens: &[String]) -> bool {
        if tokens.is_empty() {
            return false;
        }
        if tokens.len() == 1 {
            return self.approval_singles.contains(&tokens[0]);
        }
        // Multi-token: check multi-word phrases
        let joined = tokens.join(" ");
        if self.approval_multis.iter().any(|a| joined == *a || joined.starts_with(a)) {
            return true;
        }
        // Compound: first word is approval + rest is approval or filler
        if self.approval_singles.contains(&tokens[0]) {
            let tail = tokens[1..].join(" ");
            // Tail is a multi-word approval
            if self.approval_multis.iter().any(|a| tail == *a || tail.starts_with(a)) {
                return true;
            }
            // Tail is a single approval word
            if tokens.len() == 2 && self.approval_singles.contains(&tokens[1]) {
                return true;
            }
            // Approval + short tail (permissive: "sure why not", "yeah that works")
            // But not long inputs like "ok compute fibonacci" — those are commands.
            if tokens.len() <= 4 {
                return true;
            }
        }
        false
    }

    /// Check if the tokens represent a rejection.
    pub fn is_reject(&self, tokens: &[String]) -> bool {
        if tokens.is_empty() {
            return false;
        }
        if tokens.len() == 1 {
            return self.rejection_singles.contains(&tokens[0]);
        }
        // Multi-token: only match known multi-word phrases or
        // rejection word + short tail. Don't match "n queens" as rejection.
        let joined = tokens.join(" ");
        if self.rejection_multis.iter().any(|r| joined == *r || joined.starts_with(r)) {
            return true;
        }
        // First word is rejection + short tail (max 3 tokens total).
        // Longer inputs like "n queens count solutions" are commands, not rejections.
        // But NOT if the tail contains a known noun — "stop containers" is a command.
        if self.rejection_singles.contains(&tokens[0]) && tokens.len() <= 4 {
            let tail_has_noun = tokens[1..].iter().any(|t| self.nouns.contains_key(t));
            if !tail_has_noun {
                return true;
            }
        }
        false
    }

    /// Check if the tokens represent an explain request.
    /// Returns the subject if so.
    pub fn try_explain(&self, tokens: &[String]) -> Option<String> {
        if tokens.is_empty() {
            return None;
        }
        // First token must be an explain trigger
        if !self.explain_triggers.contains(&tokens[0]) {
            return None;
        }
        // Find the subject: first non-stopword token after the trigger
        let stopwords = ["is", "does", "do", "mean", "means", "work", "works", "the", "a", "an", "i"];
        for token in &tokens[1..] {
            if stopwords.contains(&token.as_str()) {
                continue;
            }
            return Some(token.clone());
        }
        None
    }

    /// Look up a (verb_action, noun_concept) pair in the concept_ops table.
    /// Returns the op/plan name if a mapping exists.
    pub fn concept_op(&self, action: &str, concept: &str) -> Option<&str> {
        let key = (action.to_lowercase(), concept.to_lowercase());
        self.concept_ops.get(&key).map(|s| s.as_str())
    }
}

// ---------------------------------------------------------------------------
// TokenClassifier implementation
// ---------------------------------------------------------------------------
impl TokenClassifier for Lexicon {
    fn classify(&self, token: &str) -> Vec<String> {
        let lower = token.to_lowercase();

        // Check explicit categories first
        if let Some(cats) = self.categories.get(&lower) {
            return cats.clone();
        }

        // Check if it looks like a filesystem path
        if is_path_token(token) {
            return vec!["path".to_string()];
        }

        // Check if it looks like a glob pattern
        if is_glob_pattern(token) {
            return vec!["pattern".to_string()];
        }

        // Unknown token — no categories
        Vec::new()
    }
}

/// Check if a token looks like a filesystem path.
fn is_path_token(token: &str) -> bool {
    token.starts_with("~/")
        || token.starts_with('/')
        || token.starts_with("$HOME")
        || token.starts_with("$PWD")
        || token.contains("://")
        || (token.contains('/') && token.len() > 2)
}

/// Check if a token looks like a glob pattern.
fn is_glob_pattern(token: &str) -> bool {
    token.starts_with("*.")
        || token.starts_with('*')
        || token.ends_with('*')
}

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

static LEXICON: OnceLock<Lexicon> = OnceLock::new();

/// Get the loaded lexicon (singleton, loaded on first call).
pub fn lexicon() -> &'static Lexicon {
    LEXICON.get_or_init(load_lexicon)
}

// ---------------------------------------------------------------------------
// Loader
// ---------------------------------------------------------------------------

fn load_lexicon() -> Lexicon {
    let yaml_str = std::fs::read_to_string("data/nl/nl_lexicon.yaml")
        .ok()
        .unwrap_or_else(|| EMBEDDED_LEXICON.to_string());

    parse_lexicon(&yaml_str).unwrap_or_else(|e| {
        eprintln!("WARN: failed to parse nl_lexicon.yaml ({}), using embedded", e);
        parse_lexicon(EMBEDDED_LEXICON).expect("embedded nl_lexicon.yaml must parse")
    })
}

fn parse_lexicon(yaml_str: &str) -> Result<Lexicon, String> {
    let raw: LexiconYaml = serde_yaml::from_str(yaml_str)
        .map_err(|e| format!("YAML parse error: {}", e))?;

    let mut categories: HashMap<String, Vec<String>> = HashMap::new();
    let mut verbs = HashMap::new();
    let mut nouns = HashMap::new();
    let mut path_nouns_map = HashMap::new();
    let mut orderings = HashMap::new();

    // Helper: add a word to a category
    let mut add_cat = |word: &str, cat: &str| {
        let lower = word.to_lowercase();
        categories.entry(lower).or_default().push(cat.to_string());
    };

    // Verbs
    for entry in &raw.verbs {
        let info = VerbInfo {
            word: entry.word.clone(),
            action: entry.action.clone(),
        };
        add_cat(&entry.word, "verb");
        verbs.insert(entry.word.to_lowercase(), info.clone());
        for syn in &entry.synonyms {
            add_cat(syn, "verb");
            verbs.insert(syn.to_lowercase(), info.clone());
        }
    }

    // Nouns
    for entry in &raw.nouns {
        let info = NounInfo {
            word: entry.word.clone(),
            concept: entry.concept.clone(),
        };
        add_cat(&entry.word, "noun");
        nouns.insert(entry.word.to_lowercase(), info.clone());
        for syn in &entry.synonyms {
            add_cat(syn, "noun");
            nouns.insert(syn.to_lowercase(), info.clone());
        }
    }

    // Path nouns (also classified as "path_noun" AND "noun" for grammar flexibility)
    for entry in &raw.path_nouns {
        let info = PathNounInfo {
            word: entry.word.clone(),
            path: entry.path.clone(),
        };
        add_cat(&entry.word, "path_noun");
        add_cat(&entry.word, "noun");
        path_nouns_map.insert(entry.word.to_lowercase(), info.clone());
        for syn in &entry.synonyms {
            add_cat(syn, "path_noun");
            add_cat(syn, "noun");
            path_nouns_map.insert(syn.to_lowercase(), info.clone());
        }
    }

    // Orderings — each word in the phrase gets classified as "ordering"
    for entry in &raw.orderings {
        let info = OrderingInfo {
            phrase: entry.phrase.clone(),
            field: entry.field.clone(),
            direction: entry.direction.clone(),
        };
        // Register each word in the phrase
        for word in entry.phrase.split_whitespace() {
            add_cat(word, "ordering");
            orderings.insert(word.to_lowercase(), info.clone());
        }
        // Register synonyms
        for syn in &entry.synonyms {
            for word in syn.split_whitespace() {
                add_cat(word, "ordering");
                orderings.insert(word.to_lowercase(), info.clone());
            }
        }
    }

    // Prepositions
    let prepositions: std::collections::HashSet<String> = raw.prepositions.iter()
        .map(|w| { add_cat(w, "preposition"); w.to_lowercase() })
        .collect();

    // Possessives
    let possessives: std::collections::HashSet<String> = raw.possessives.iter()
        .map(|w| { add_cat(w, "possessive"); w.to_lowercase() })
        .collect();

    // Determiners
    let determiners: std::collections::HashSet<String> = raw.determiners.iter()
        .map(|w| { add_cat(w, "determiner"); w.to_lowercase() })
        .collect();

    // Quantifiers
    let quantifiers: std::collections::HashSet<String> = raw.quantifiers.iter()
        .map(|w| { add_cat(w, "quantifier"); w.to_lowercase() })
        .collect();

    // Conjunctions
    let conjunctions: std::collections::HashSet<String> = raw.conjunctions.iter()
        .map(|w| { add_cat(w, "conjunction"); w.to_lowercase() })
        .collect();

    // Fillers
    let fillers: std::collections::HashSet<String> = raw.fillers.iter()
        .map(|w| { add_cat(w, "filler"); w.to_lowercase() })
        .collect();

    // Approvals
    let mut approval_singles = std::collections::HashSet::new();
    let mut approval_multis = Vec::new();
    if let Some(ref approvals) = raw.approvals {
        for word in &approvals.single {
            let lower = word.to_lowercase();
            add_cat(&lower, "approval");
            approval_singles.insert(lower);
        }
        for phrase in &approvals.multi {
            approval_multis.push(phrase.to_lowercase());
        }
    }

    // Rejections
    let mut rejection_singles = std::collections::HashSet::new();
    let mut rejection_multis = Vec::new();
    if let Some(ref rejections) = raw.rejections {
        for word in &rejections.single {
            let lower = word.to_lowercase();
            add_cat(&lower, "rejection");
            rejection_singles.insert(lower);
        }
        for phrase in &rejections.multi {
            rejection_multis.push(phrase.to_lowercase());
        }
    }

    // Explain triggers
    let explain_triggers: std::collections::HashSet<String> = raw.explain_triggers.iter()
        .map(|w| { add_cat(w, "explain_trigger"); w.to_lowercase() })
        .collect();

    // Deduplicate categories for each word
    for cats in categories.values_mut() {
        cats.sort();
        cats.dedup();
    }

    // Phrase groups
    let phrase_groups: Vec<PhraseGroup> = raw.phrase_groups.iter()
        .filter(|pg| pg.skeleton.len() >= 2)
        .map(|pg| PhraseGroup {
            skeleton: pg.skeleton.iter().map(|w| w.to_lowercase()).collect(),
            canonical: pg.canonical.to_lowercase(),
        })
        .collect();

    // Concept ops: (action, concept) → op_name
    let concept_ops: HashMap<(String, String), String> = raw.concept_ops.iter()
        .map(|entry| (
            (entry.action.to_lowercase(), entry.concept.to_lowercase()),
            entry.op.clone(),
        ))
        .collect();

    Ok(Lexicon {
        categories,
        verbs,
        nouns,
        path_nouns: path_nouns_map,
        orderings,
        phrase_groups,
        fillers,
        prepositions,
        possessives,
        determiners,
        quantifiers,
        conjunctions,
        approval_singles,
        approval_multis,
        rejection_singles,
        rejection_multis,
        explain_triggers,
        concept_ops,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexicon_loads() {
        let lex = lexicon();
        assert!(!lex.verbs.is_empty(), "verbs should not be empty");
        assert!(!lex.nouns.is_empty(), "nouns should not be empty");
        assert!(!lex.path_nouns.is_empty(), "path_nouns should not be empty");
        assert!(!lex.orderings.is_empty(), "orderings should not be empty");
    }

    #[test]
    fn test_verb_find_is_select() {
        let lex = lexicon();
        let info = lex.verbs.get("find").expect("find should be a verb");
        assert_eq!(info.action, "select");
    }

    #[test]
    fn test_verb_search_is_search_text() {
        let lex = lexicon();
        let info = lex.verbs.get("search").expect("search should be a verb");
        assert_eq!(info.action, "search_text");
    }

    #[test]
    fn test_noun_comics_is_comic_issue_archive() {
        let lex = lexicon();
        let info = lex.nouns.get("comics").expect("comics should be a noun");
        assert_eq!(info.concept, "comic_issue_archive");
    }

    #[test]
    fn test_noun_synonym_manga() {
        let lex = lexicon();
        let info = lex.nouns.get("manga").expect("manga should be a noun synonym");
        assert_eq!(info.concept, "comic_issue_archive");
    }

    #[test]
    fn test_path_noun_downloads() {
        let lex = lexicon();
        let info = lex.path_nouns.get("downloads").expect("downloads should be a path_noun");
        assert_eq!(info.path, "~/Downloads");
    }

    #[test]
    fn test_ordering_newest() {
        let lex = lexicon();
        let info = lex.orderings.get("newest").expect("newest should be an ordering");
        assert_eq!(info.field, "modification_time");
        assert_eq!(info.direction, "descending");
    }

    #[test]
    fn test_classify_verb() {
        let lex = lexicon();
        let cats = lex.classify("find");
        assert!(cats.contains(&"verb".to_string()), "find should classify as verb: {:?}", cats);
    }

    #[test]
    fn test_classify_noun() {
        let lex = lexicon();
        let cats = lex.classify("comics");
        assert!(cats.contains(&"noun".to_string()), "comics should classify as noun: {:?}", cats);
    }

    #[test]
    fn test_classify_preposition() {
        let lex = lexicon();
        let cats = lex.classify("in");
        assert!(cats.contains(&"preposition".to_string()), "in should classify as preposition: {:?}", cats);
    }

    #[test]
    fn test_classify_path() {
        let lex = lexicon();
        let cats = lex.classify("~/Downloads");
        assert!(cats.contains(&"path".to_string()), "~/Downloads should classify as path: {:?}", cats);
    }

    #[test]
    fn test_classify_glob() {
        let lex = lexicon();
        let cats = lex.classify("*.pdf");
        assert!(cats.contains(&"pattern".to_string()), "*.pdf should classify as pattern: {:?}", cats);
    }

    #[test]
    fn test_classify_unknown() {
        let lex = lexicon();
        let cats = lex.classify("xyzzyplugh");
        assert!(cats.is_empty(), "unknown word should have no categories: {:?}", cats);
    }

    #[test]
    fn test_classify_case_insensitive() {
        let lex = lexicon();
        let cats = lex.classify("Find");
        assert!(cats.contains(&"verb".to_string()), "Find (capitalized) should classify as verb: {:?}", cats);
    }

    #[test]
    fn test_filler_words_loaded() {
        let lex = lexicon();
        assert!(lex.fillers.contains("please"), "please should be a filler");
        assert!(lex.fillers.contains("just"), "just should be a filler");
    }

    #[test]
    fn test_parse_embedded_always_works() {
        let result = parse_lexicon(EMBEDDED_LEXICON);
        assert!(result.is_ok(), "embedded lexicon must parse: {:?}", result.err());
    }

    #[test]
    fn test_parse_malformed_yaml_returns_error() {
        let result = parse_lexicon("not: valid: yaml: [[[");
        assert!(result.is_err());
    }

    #[test]
    fn test_multi_category_word() {
        // "my" should be both possessive and determiner
        let lex = lexicon();
        let cats = lex.classify("my");
        assert!(cats.contains(&"possessive".to_string()), "my should be possessive: {:?}", cats);
    }

    #[test]
    fn test_path_noun_also_classified_as_noun() {
        let lex = lexicon();
        let cats = lex.classify("downloads");
        assert!(cats.contains(&"path_noun".to_string()), "downloads should be path_noun: {:?}", cats);
        assert!(cats.contains(&"noun".to_string()), "downloads should also be noun: {:?}", cats);
    }

    #[test]
    fn test_verb_zip_is_compress() {
        let lex = lexicon();
        let info = lex.verbs.get("zip").expect("zip should be a verb");
        assert_eq!(info.action, "compress");
    }

    #[test]
    fn test_verb_extract_is_decompress() {
        let lex = lexicon();
        let info = lex.verbs.get("extract").expect("extract should be a verb");
        assert_eq!(info.action, "decompress");
    }

    #[test]
    fn test_verb_sort_is_order() {
        let lex = lexicon();
        let info = lex.verbs.get("sort").expect("sort should be a verb");
        assert_eq!(info.action, "order");
    }

    #[test]
    fn test_phrase_groups_loaded() {
        let lex = lexicon();
        assert!(!lex.phrase_groups.is_empty(), "phrase_groups should not be empty");
        let make_list = lex.phrase_groups.iter()
            .find(|pg| pg.skeleton == vec!["make", "list"])
            .expect("should have [make, list] phrase group");
        assert_eq!(make_list.canonical, "list");
    }

    #[test]
    fn test_phrase_groups_all_have_2_plus_words() {
        let lex = lexicon();
        for pg in &lex.phrase_groups {
            assert!(pg.skeleton.len() >= 2,
                "phrase group skeleton must have 2+ words: {:?}", pg.skeleton);
        }
    }

    // -- Approval/Rejection/Explain tests --

    #[test]
    fn test_approve_lgtm() {
        let lex = lexicon();
        assert!(lex.is_approve(&["lgtm".into()]));
    }

    #[test]
    fn test_approve_yes() {
        let lex = lexicon();
        assert!(lex.is_approve(&["yes".into()]));
    }

    #[test]
    fn test_approve_sounds_good() {
        let lex = lexicon();
        assert!(lex.is_approve(&["sounds".into(), "good".into()]));
    }

    #[test]
    fn test_approve_ok_lgtm() {
        let lex = lexicon();
        assert!(lex.is_approve(&["ok".into(), "lgtm".into()]));
    }

    #[test]
    fn test_approve_sure_why_not() {
        let lex = lexicon();
        assert!(lex.is_approve(&["sure".into(), "why".into(), "not".into()]));
    }

    #[test]
    fn test_approve_y() {
        let lex = lexicon();
        assert!(lex.is_approve(&["y".into()]));
    }

    #[test]
    fn test_reject_no() {
        let lex = lexicon();
        assert!(lex.is_reject(&["no".into()]));
    }

    #[test]
    fn test_reject_scratch_that() {
        let lex = lexicon();
        assert!(lex.is_reject(&["scratch".into(), "that".into()]));
    }

    #[test]
    fn test_reject_nah_i_am_good() {
        let lex = lexicon();
        assert!(lex.is_reject(&["nah".into(), "i".into(), "am".into(), "good".into()]));
    }

    #[test]
    fn test_explain_filter() {
        let lex = lexicon();
        let subject = lex.try_explain(&["explain".into(), "filter".into()]);
        assert_eq!(subject, Some("filter".into()));
    }

    #[test]
    fn test_explain_what_is_walk_tree() {
        let lex = lexicon();
        let subject = lex.try_explain(&["what".into(), "is".into(), "walk_tree".into()]);
        assert_eq!(subject, Some("walk_tree".into()));
    }

    #[test]
    fn test_not_approve_when_empty() {
        let lex = lexicon();
        assert!(!lex.is_approve(&[]));
    }

    #[test]
    fn test_not_reject_when_empty() {
        let lex = lexicon();
        assert!(!lex.is_reject(&[]));
    }
}
