//! Terminal UI primitives — colors, icons, and formatting helpers.
//!
//! Zero external dependencies. Uses raw ANSI escape codes.
//! Respects the `NO_COLOR` environment variable (https://no-color.org/).

use std::time::Duration;
use std::fmt;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Color support detection
// ---------------------------------------------------------------------------

/// Returns `true` if color output is enabled.
/// Disabled when `NO_COLOR` env var is set (any value) or `TERM=dumb`.
pub fn color_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        if let Ok(term) = std::env::var("TERM") {
            if term == "dumb" {
                return false;
            }
        }
        true
    })
}

// ---------------------------------------------------------------------------
// ANSI escape helpers
// ---------------------------------------------------------------------------

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const ITALIC: &str = "\x1b[3m";
const UNDERLINE: &str = "\x1b[4m";

// Foreground colors
const FG_BLACK: &str = "\x1b[30m";
const FG_RED: &str = "\x1b[31m";
const FG_GREEN: &str = "\x1b[32m";
const FG_YELLOW: &str = "\x1b[33m";
const FG_BLUE: &str = "\x1b[34m";
const FG_MAGENTA: &str = "\x1b[35m";
const FG_CYAN: &str = "\x1b[36m";
const FG_WHITE: &str = "\x1b[37m";

// Bright foreground colors
const FG_BRIGHT_BLACK: &str = "\x1b[90m";
const FG_BRIGHT_RED: &str = "\x1b[91m";
const FG_BRIGHT_GREEN: &str = "\x1b[92m";
const FG_BRIGHT_YELLOW: &str = "\x1b[93m";
const FG_BRIGHT_BLUE: &str = "\x1b[94m";
const FG_BRIGHT_MAGENTA: &str = "\x1b[95m";
const FG_BRIGHT_CYAN: &str = "\x1b[96m";
const FG_BRIGHT_WHITE: &str = "\x1b[97m";

// Background colors
const BG_RED: &str = "\x1b[41m";
const BG_GREEN: &str = "\x1b[42m";
const BG_YELLOW: &str = "\x1b[43m";
const BG_BLUE: &str = "\x1b[44m";
const BG_MAGENTA: &str = "\x1b[45m";
const BG_CYAN: &str = "\x1b[46m";
const BG_BRIGHT_BLACK: &str = "\x1b[100m";

/// Apply an ANSI style to text. Returns plain text if color is disabled.
fn styled(codes: &[&str], text: &str) -> String {
    if !color_enabled() || codes.is_empty() {
        return text.to_string();
    }
    let prefix: String = codes.iter().copied().collect();
    format!("{}{}{}", prefix, text, RESET)
}

// ---------------------------------------------------------------------------
// Public style functions
// ---------------------------------------------------------------------------

pub fn bold(text: &str) -> String { styled(&[BOLD], text) }
pub fn dim(text: &str) -> String { styled(&[DIM], text) }
pub fn italic(text: &str) -> String { styled(&[ITALIC], text) }
pub fn underline(text: &str) -> String { styled(&[UNDERLINE], text) }

pub fn red(text: &str) -> String { styled(&[FG_RED], text) }
pub fn green(text: &str) -> String { styled(&[FG_GREEN], text) }
pub fn yellow(text: &str) -> String { styled(&[FG_YELLOW], text) }
pub fn blue(text: &str) -> String { styled(&[FG_BLUE], text) }
pub fn magenta(text: &str) -> String { styled(&[FG_MAGENTA], text) }
pub fn cyan(text: &str) -> String { styled(&[FG_CYAN], text) }
pub fn white(text: &str) -> String { styled(&[FG_WHITE], text) }

pub fn bright_black(text: &str) -> String { styled(&[FG_BRIGHT_BLACK], text) }
pub fn bright_red(text: &str) -> String { styled(&[FG_BRIGHT_RED], text) }
pub fn bright_green(text: &str) -> String { styled(&[FG_BRIGHT_GREEN], text) }
pub fn bright_yellow(text: &str) -> String { styled(&[FG_BRIGHT_YELLOW], text) }
pub fn bright_blue(text: &str) -> String { styled(&[FG_BRIGHT_BLUE], text) }
pub fn bright_magenta(text: &str) -> String { styled(&[FG_BRIGHT_MAGENTA], text) }
pub fn bright_cyan(text: &str) -> String { styled(&[FG_BRIGHT_CYAN], text) }
pub fn bright_white(text: &str) -> String { styled(&[FG_BRIGHT_WHITE], text) }

pub fn bold_red(text: &str) -> String { styled(&[BOLD, FG_RED], text) }
pub fn bold_green(text: &str) -> String { styled(&[BOLD, FG_GREEN], text) }
pub fn bold_yellow(text: &str) -> String { styled(&[BOLD, FG_YELLOW], text) }
pub fn bold_blue(text: &str) -> String { styled(&[BOLD, FG_BLUE], text) }
pub fn bold_magenta(text: &str) -> String { styled(&[BOLD, FG_MAGENTA], text) }
pub fn bold_cyan(text: &str) -> String { styled(&[BOLD, FG_CYAN], text) }
pub fn bold_white(text: &str) -> String { styled(&[BOLD, FG_WHITE], text) }

pub fn dim_white(text: &str) -> String { styled(&[DIM, FG_WHITE], text) }
pub fn dim_cyan(text: &str) -> String { styled(&[DIM, FG_CYAN], text) }

/// White text on colored background — for badges/tags.
pub fn badge_green(text: &str) -> String { styled(&[BOLD, FG_BLACK, BG_GREEN], text) }
pub fn badge_red(text: &str) -> String { styled(&[BOLD, FG_WHITE, BG_RED], text) }
pub fn badge_yellow(text: &str) -> String { styled(&[BOLD, FG_BLACK, BG_YELLOW], text) }
pub fn badge_blue(text: &str) -> String { styled(&[BOLD, FG_WHITE, BG_BLUE], text) }
pub fn badge_magenta(text: &str) -> String { styled(&[BOLD, FG_WHITE, BG_MAGENTA], text) }
pub fn badge_cyan(text: &str) -> String { styled(&[BOLD, FG_BLACK, BG_CYAN], text) }
pub fn badge_gray(text: &str) -> String { styled(&[FG_WHITE, BG_BRIGHT_BLACK], text) }

// ---------------------------------------------------------------------------
// Geometric icons — flat, modern, no bubbly emojis
// ---------------------------------------------------------------------------

/// Icons used throughout the CLI. Flat geometric style.
pub mod icon {
    // Status
    pub const OK: &str = "✓";
    pub const FAIL: &str = "✗";
    pub const WARN: &str = "△";
    pub const INFO: &str = "◆";
    pub const PENDING: &str = "○";
    pub const ACTIVE: &str = "●";

    // Navigation / flow
    pub const ARROW_RIGHT: &str = "→";
    pub const ARROW_LEFT: &str = "←";
    pub const ARROW_DOWN: &str = "↓";
    pub const PIPE: &str = "│";
    pub const STEP: &str = "▸";
    pub const CHAIN: &str = "▸";

    // Structural
    pub const DIAMOND: &str = "◇";
    pub const DIAMOND_FILL: &str = "◆";
    pub const SQUARE: &str = "▪";
    pub const SQUARE_OPEN: &str = "▫";
    pub const BLOCK: &str = "▰";
    pub const BLOCK_OPEN: &str = "▱";
    pub const DOT: &str = "·";
    pub const BULLET: &str = "▸";

    // Comparison / analysis
    pub const SIMILARITY: &str = "↔";
    pub const CONTRAST: &str = "⊘";
    pub const TENSION: &str = "⊗";
    pub const EVIDENCE: &str = "▪";
    pub const CLAIM: &str = "◇";
    pub const UNCERTAINTY: &str = "△";
    pub const INFERENCE: &str = "⊢";
    pub const CONFLICT: &str = "⊥";

    // Tree drawing
    pub const TREE_BRANCH: &str = "├";
    pub const TREE_LAST: &str = "└";
    pub const TREE_PIPE: &str = "│";
    pub const TREE_DASH: &str = "─";

    // Misc
    pub const SECTION: &str = "▰▰▰";
    pub const PROMPT: &str = "◆";
    pub const LAMBDA: &str = "λ";
    pub const GEAR: &str = "⚙";
}

// ---------------------------------------------------------------------------
// Formatting primitives
// ---------------------------------------------------------------------------

/// Print a compact banner.
///
/// ```text
/// ▰ CADMUS v0.6.0 — Plan DSL
/// ```
pub fn banner(name: &str, version: &str, subtitle: &str) -> String {
    if subtitle.is_empty() {
        format!("{} {} {}",
            bold_cyan(&icon::SECTION.to_string()),
            bold_white(name),
            dim(version),
        )
    } else {
        format!("{} {} {} {} {}",
            bold_cyan(&icon::SECTION.to_string()),
            bold_white(name),
            dim(version),
            dim("—"),
            dim(subtitle),
        )
    }
}

/// Section header with a horizontal rule.
///
/// ```text
/// ── Strategy 1: Comparison ──────────────────
/// ```
pub fn section(title: &str) -> String {
    let rule_len = 48usize.saturating_sub(title.len() + 6);
    let rule: String = "─".repeat(rule_len);
    format!("{} {} {}", dim("──"), bold_white(title), dim(&rule))
}

/// Sub-section header (lighter).
pub fn subsection(title: &str) -> String {
    format!("  {} {}", dim("─"), bold(title))
}

/// A key-value pair, key in color, value plain.
///
/// ```text
///   goal  Produce a structured comparison
/// ```
pub fn kv(key: &str, value: &str) -> String {
    format!("  {}  {}", cyan(key), value)
}

/// A key-value pair with dim value.
pub fn kv_dim(key: &str, value: &str) -> String {
    format!("  {}  {}", cyan(key), dim(value))
}

/// A numbered step in a pipeline.
///
/// ```text
///   1 ▸ walk_tree  Dir(a) → Seq(Entry(Name, a))
/// ```
pub fn step(index: usize, name: &str, detail: &str) -> String {
    let num = dim(&format!("{:>3}", index));
    if detail.is_empty() {
        format!("{} {} {}", num, dim(icon::STEP), bold(name))
    } else {
        format!("{} {} {}  {}", num, dim(icon::STEP), bold(name), dim(detail))
    }
}

/// A step with an each-mode marker.
pub fn step_each(index: usize, name: &str, detail: &str) -> String {
    let num = dim(&format!("{:>3}", index));
    let each_tag = dim_cyan("[each]");
    if detail.is_empty() {
        format!("{} {} {} {}", num, dim(icon::STEP), bold(name), each_tag)
    } else {
        format!("{} {} {} {} {}", num, dim(icon::STEP), bold(name), each_tag, dim(detail))
    }
}

/// Status badge — inline colored tag.
///
/// ```text
///  ✓ COMPILED   ✗ FAILED   ○ PENDING
/// ```
pub fn status_ok(label: &str) -> String {
    format!("{} {}", bold_green(icon::OK), bold_green(label))
}

pub fn status_fail(label: &str) -> String {
    format!("{} {}", bold_red(icon::FAIL), bold_red(label))
}

pub fn status_warn(label: &str) -> String {
    format!("{} {}", bold_yellow(icon::WARN), bold_yellow(label))
}

pub fn status_info(label: &str) -> String {
    format!("{} {}", bold_cyan(icon::INFO), bold_cyan(label))
}

pub fn status_pending(label: &str) -> String {
    format!("{} {}", dim(icon::PENDING), dim(label))
}

pub fn status_active(label: &str) -> String {
    format!("{} {}", bold_blue(icon::ACTIVE), bold_blue(label))
}

/// Format a duration for display.
///
/// - Under 1 second: `"142ms"`
/// - 1 second or more: `"1.3s"`
/// - 60 seconds or more: `"1m 5.2s"`
pub fn format_duration(d: Duration) -> String {
    let total_ms = d.as_millis();
    if total_ms < 1000 {
        format!("{}ms", total_ms)
    } else {
        let secs = d.as_secs_f64();
        if secs < 60.0 {
            format!("{:.1}s", secs)
        } else {
            let mins = (secs / 60.0).floor() as u64;
            let rem = secs - (mins as f64 * 60.0);
            format!("{}m {:.1}s", mins, rem)
        }
    }
}

/// Format a timing label with a duration, styled dim.
pub fn timing(label: &str, d: Duration) -> String {
    format!("  {} {}", dim(label), dim(&format_duration(d)))
}

/// A code/script block with dim border.
///
/// ```text
///   ┌─────────
///   │ #!/bin/sh
///   │ set -e
///   └─────────
/// ```
pub fn code_block(code: &str) -> String {
    let border = dim("─────────────────────────────────────────");
    let mut out = String::new();
    out.push_str(&format!("  {}{}\n", dim("┌"), border));
    for line in code.lines() {
        out.push_str(&format!("  {} {}\n", dim("│"), line));
    }
    out.push_str(&format!("  {}{}", dim("└"), border));
    out
}

/// A code block with line numbers.
pub fn code_block_numbered(code: &str, start_line: usize) -> String {
    let border = dim("─────────────────────────────────────────");
    let mut out = String::new();
    out.push_str(&format!("  {}{}\n", dim("┌"), border));
    for (i, line) in code.lines().enumerate() {
        let num = dim(&format!("{:>4}", start_line + i));
        out.push_str(&format!("  {} {} {}\n", dim("│"), num, line));
    }
    out.push_str(&format!("  {}{}", dim("└"), border));
    out
}

/// Dim YAML block (for plan plans shown to user).
pub fn plan_block(text: &str) -> String {
    let border = dim("─────────────────────────────────────────");
    let mut out = String::new();
    out.push_str(&format!("  {}{}\n", dim("┌"), border));
    for line in text.lines() {
        out.push_str(&format!("  {} {}\n", dim("│"), dim_white(line)));
    }
    out.push_str(&format!("  {}{}", dim("└"), border));
    out
}

/// Tree item (not last).
///
/// ```text
///   ├─ item text
/// ```
pub fn tree_item(text: &str) -> String {
    format!("  {}{}  {}", dim(icon::TREE_BRANCH), dim(icon::TREE_DASH), text)
}

/// Tree item (last in group).
pub fn tree_last(text: &str) -> String {
    format!("  {}{}  {}", dim(icon::TREE_LAST), dim(icon::TREE_DASH), text)
}

/// Tree continuation line (for multi-line content under a tree item).
pub fn tree_cont(text: &str) -> String {
    format!("  {}   {}", dim(icon::TREE_PIPE), text)
}

/// Blank tree continuation (just the pipe).
pub fn tree_blank() -> String {
    format!("  {}", dim(icon::TREE_PIPE))
}

/// Indented bullet point.
pub fn bullet(text: &str) -> String {
    format!("  {} {}", dim(icon::BULLET), text)
}

/// Error message.
pub fn error(msg: &str) -> String {
    format!("{} {}", bold_red(icon::FAIL), red(msg))
}

/// Warning message.
pub fn warning(msg: &str) -> String {
    format!("{} {}", bold_yellow(icon::WARN), yellow(msg))
}

/// Info message.
pub fn info(msg: &str) -> String {
    format!("{} {}", bold_cyan(icon::INFO), cyan(msg))
}

/// Success message.
pub fn success(msg: &str) -> String {
    format!("{} {}", bold_green(icon::OK), green(msg))
}

/// Horizontal rule.
pub fn rule() -> String {
    dim(&"─".repeat(52))
}

/// Compact prompt string for interactive modes.
pub fn prompt() -> String {
    if color_enabled() {
        format!("{}{}{} ", BOLD, FG_CYAN, icon::PROMPT)
    } else {
        format!("{} ", icon::PROMPT)
    }
}

/// Agent session prompt (distinct color from NL chat prompt).
pub fn agent_prompt() -> String {
    if color_enabled() {
        format!("{}{}{} ", BOLD, FG_BLUE, icon::PROMPT)
    } else {
        format!("{} ", icon::PROMPT)
    }
}

/// Reset code (for use after prompt where we don't want to reset inline).
pub fn reset() -> &'static str {
    if color_enabled() { RESET } else { "" }
}

// ---------------------------------------------------------------------------
// Axis / comparison formatting helpers
// ---------------------------------------------------------------------------

/// Axis header for comparison output.
///
/// ```text
///   ┌── ideology ──────────────────────────
/// ```
pub fn axis_header(name: &str) -> String {
    let rule_len = 44usize.saturating_sub(name.len() + 2);
    let rule: String = "─".repeat(rule_len);
    format!("  {}{} {} {}", dim("┌"), dim("──"), bold_cyan(name), dim(&rule))
}

/// Axis footer.
pub fn axis_footer() -> String {
    format!("  {}{}", dim("└"), dim(&"─".repeat(48)))
}

/// Claim line inside an axis.
pub fn claim(entity: &str, content: &str) -> String {
    format!("  {}  {} {} {}", dim(icon::TREE_PIPE), cyan(icon::CLAIM),
        badge_gray(&format!(" {} ", entity.to_uppercase())), content)
}

/// Evidence line inside an axis.
pub fn evidence(entity: &str, content: &str) -> String {
    format!("  {}  {} {} {}", dim(icon::TREE_PIPE), dim(icon::EVIDENCE),
        dim(&format!("({})", entity)), dim(content))
}

/// Similarity line.
pub fn similarity(content: &str) -> String {
    format!("  {}  {} {}", dim(icon::TREE_PIPE), green(icon::SIMILARITY), content)
}

/// Contrast line.
pub fn contrast_line(content: &str) -> String {
    format!("  {}  {} {}", dim(icon::TREE_PIPE), yellow(icon::CONTRAST), content)
}

/// Uncertainty line.
pub fn uncertainty(content: &str) -> String {
    format!("  {}  {} {}", dim(icon::TREE_PIPE), magenta(icon::UNCERTAINTY), dim(content))
}

/// Summary line inside an axis.
pub fn summary_line(content: &str) -> String {
    format!("  {}  {}", dim(icon::TREE_PIPE), dim(content))
}

/// Gap/warning line inside an axis.
pub fn gap_line(content: &str) -> String {
    format!("  {}  {} {}", dim(icon::TREE_PIPE), bold_yellow(icon::WARN), yellow(content))
}

/// Inference line.
pub fn inference_line(content: &str) -> String {
    format!("  {} {}", cyan(icon::INFERENCE), content)
}

/// Conflict line.
pub fn conflict_line(content: &str) -> String {
    format!("  {} {}", bold_red(icon::CONFLICT), red(content))
}

// ---------------------------------------------------------------------------
// Write helpers (for Display impls that write to fmt::Formatter)
// ---------------------------------------------------------------------------

/// Write a step line to a formatter (for Display impls).
pub fn write_step(f: &mut fmt::Formatter<'_>, index: usize, name: &str, is_each: bool, detail: &str) -> fmt::Result {
    if is_each {
        writeln!(f, "{}", step_each(index, name, detail))
    } else {
        writeln!(f, "{}", step(index, name, detail))
    }
}

/// Write a key-value line to a formatter.
pub fn write_kv(f: &mut fmt::Formatter<'_>, key: &str, value: &str) -> fmt::Result {
    writeln!(f, "{}", kv(key, value))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_styled_with_color() {
        // Force color for test (we can't easily control OnceLock, but we test the function directly)
        let result = styled(&[BOLD, FG_RED], "hello");
        // When color is enabled, should contain ANSI codes
        // When disabled, should be plain text
        // Either way, the text content should be present
        assert!(result.contains("hello"));
    }

    #[test]
    fn test_styled_empty_codes() {
        let result = styled(&[], "hello");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_styled_empty_text() {
        let result = styled(&[BOLD], "");
        // Should not panic on empty text
        assert!(result.is_empty() || result.contains(RESET));
    }

    #[test]
    fn test_banner_with_subtitle() {
        let b = banner("CADMUS", "v0.6.0", "Plan DSL");
        assert!(b.contains("CADMUS"));
        assert!(b.contains("v0.6.0"));
        assert!(b.contains("Plan DSL"));
    }

    #[test]
    fn test_banner_without_subtitle() {
        let b = banner("CADMUS", "v0.6.0", "");
        assert!(b.contains("CADMUS"));
        assert!(b.contains("v0.6.0"));
        assert!(!b.contains("—") || !color_enabled());
    }

    #[test]
    fn test_section_header() {
        let s = section("Strategy 1: Comparison");
        assert!(s.contains("Strategy 1: Comparison"));
        assert!(s.contains("─"));
    }

    #[test]
    fn test_kv_pair() {
        let k = kv("goal", "Compare entities");
        assert!(k.contains("goal"));
        assert!(k.contains("Compare entities"));
    }

    #[test]
    fn test_step_formatting() {
        let s = step(1, "walk_tree", "Dir(a) → Seq(Entry(Name, a))");
        assert!(s.contains("1"));
        assert!(s.contains("walk_tree"));
        assert!(s.contains("Dir(a)"));
    }

    #[test]
    fn test_step_each_formatting() {
        let s = step_each(2, "read_file", "File(a) → a");
        assert!(s.contains("2"));
        assert!(s.contains("read_file"));
        assert!(s.contains("[each]"));
    }

    #[test]
    fn test_step_no_detail() {
        let s = step(1, "walk_tree", "");
        assert!(s.contains("walk_tree"));
    }

    #[test]
    fn test_status_badges() {
        let ok = status_ok("COMPILED");
        assert!(ok.contains("COMPILED"));
        assert!(ok.contains(icon::OK));

        let fail = status_fail("FAILED");
        assert!(fail.contains("FAILED"));
        assert!(fail.contains(icon::FAIL));
    }

    #[test]
    fn test_code_block() {
        let block = code_block("#!/bin/sh\nset -e\necho hello");
        assert!(block.contains("#!/bin/sh"));
        assert!(block.contains("set -e"));
        assert!(block.contains("echo hello"));
        assert!(block.contains("┌"));
        assert!(block.contains("└"));
    }

    #[test]
    fn test_code_block_empty() {
        let block = code_block("");
        // Should not panic, should have borders
        assert!(block.contains("┌"));
        assert!(block.contains("└"));
    }

    #[test]
    fn test_code_block_numbered() {
        let block = code_block_numbered("fn main() {\n    println!(\"hi\");\n}", 1);
        assert!(block.contains("1"));
        assert!(block.contains("fn main()"));
    }

    #[test]
    fn test_plan_block() {
        let block = plan_block("test-plan:\n  steps:\nsteps:\n  - walk_tree");
        assert!(block.contains("test-plan:"));
        assert!(block.contains("walk_tree"));
    }

    #[test]
    fn test_tree_items() {
        let item = tree_item("first item");
        assert!(item.contains("├"));
        assert!(item.contains("first item"));

        let last = tree_last("last item");
        assert!(last.contains("└"));
        assert!(last.contains("last item"));

        let cont = tree_cont("continuation");
        assert!(cont.contains("│"));
        assert!(cont.contains("continuation"));
    }

    #[test]
    fn test_error_warning_info_success() {
        let e = error("something broke");
        assert!(e.contains("something broke"));
        assert!(e.contains(icon::FAIL));

        let w = warning("be careful");
        assert!(w.contains("be careful"));

        let i = info("note this");
        assert!(i.contains("note this"));

        let s = success("all good");
        assert!(s.contains("all good"));
    }

    #[test]
    fn test_axis_header_footer() {
        let h = axis_header("ideology");
        assert!(h.contains("ideology"));
        assert!(h.contains("┌"));

        let f = axis_footer();
        assert!(f.contains("└"));
    }

    #[test]
    fn test_claim_formatting() {
        let c = claim("putin", "Autocratic leadership style");
        assert!(c.contains("PUTIN"));
        assert!(c.contains("Autocratic leadership style"));
    }

    #[test]
    fn test_evidence_formatting() {
        let e = evidence("stalin", "Historical records show...");
        assert!(e.contains("stalin"));
        assert!(e.contains("Historical records show..."));
    }

    #[test]
    fn test_similarity_contrast_uncertainty() {
        let s = similarity("Both used propaganda");
        assert!(s.contains("Both used propaganda"));

        let c = contrast_line("Different eras");
        assert!(c.contains("Different eras"));

        let u = uncertainty("Exact death toll disputed");
        assert!(u.contains("Exact death toll disputed"));
    }

    #[test]
    fn test_rule() {
        let r = rule();
        assert!(r.contains("─"));
    }

    #[test]
    fn test_prompt_string() {
        let p = prompt();
        assert!(p.contains(icon::PROMPT));
    }

    #[test]
    fn test_bullet() {
        let b = bullet("an item");
        assert!(b.contains("an item"));
        assert!(b.contains(icon::BULLET));
    }

    #[test]
    fn test_long_text_no_panic() {
        let long = "x".repeat(10_000);
        let _ = bold(&long);
        let _ = code_block(&long);
        let _ = section(&long);
        let _ = kv("key", &long);
        let _ = claim("entity", &long);
    }

    #[test]
    fn test_badge_functions() {
        let g = badge_green(" OK ");
        assert!(g.contains("OK"));
        let r = badge_red(" ERR ");
        assert!(r.contains("ERR"));
    }

    #[test]
    fn test_inference_conflict_lines() {
        let i = inference_line("A implies B");
        assert!(i.contains("A implies B"));
        assert!(i.contains(icon::INFERENCE));

        let c = conflict_line("X contradicts Y");
        assert!(c.contains("X contradicts Y"));
        assert!(c.contains(icon::CONFLICT));
    }

    #[test]
    fn test_gap_line() {
        let g = gap_line("Missing evidence for axis");
        assert!(g.contains("Missing evidence"));
        assert!(g.contains(icon::WARN));
    }

    #[test]
    fn test_subsection() {
        let s = subsection("Code Smells");
        assert!(s.contains("Code Smells"));
    }
}
