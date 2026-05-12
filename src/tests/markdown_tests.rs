use crate::ui::markdown::{markdown_to_styled, word_wrap};
use crate::ui::utils::display_width;
use crossterm::style::Color;

// ── word_wrap ───────────────────────────────────────────────────────

#[test]
fn wrap_fits_within_width() {
    let result = word_wrap("hello world", 20);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "hello world");
}

#[test]
fn wrap_empty() {
    let result = word_wrap("", 10);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "");
}

#[test]
fn wrap_zero_width() {
    let result = word_wrap("hello world", 0);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "hello world");
}

#[test]
fn wrap_at_word_boundary() {
    let result = word_wrap("hello world foo bar", 12);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], "hello world");
    assert_eq!(result[1], "foo bar");
}

#[test]
fn wrap_long_single_word() {
    let result = word_wrap("supercalifragilisticexpialidocious", 10);
    assert!(result.len() > 1);
    for line in &result {
        assert!(display_width(line) <= 10);
    }
}

#[test]
fn wrap_preserves_bullet() {
    let result = word_wrap("  • hello world this is a test with a longer bullet", 20);
    assert!(result[0].contains('•'));
}

#[test]
fn wrap_multiple_spaces() {
    let result = word_wrap("a  b  c", 10);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "a  b  c");
}

// ── markdown_to_styled: inline code ─────────────────────────────────

#[test]
fn inline_code_styled() {
    let styled = markdown_to_styled("Hello `code` world", 80);
    let joined: String = styled
        .iter()
        .map(|e| e.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        joined.contains("`code`"),
        "inline code should have backticks: {joined}"
    );
    assert!(
        joined.contains("Hello"),
        "prose before code should be present: {joined}"
    );
    assert!(
        joined.contains("world"),
        "prose after code should be present: {joined}"
    );
}

#[test]
fn multiple_inline_codes_no_duplication() {
    let styled = markdown_to_styled("foo `a` bar `b` baz", 80);
    let joined: String = styled
        .iter()
        .map(|e| e.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        joined.matches("foo").count(),
        1,
        "prose before first code must not duplicate: {joined}"
    );
    assert_eq!(
        joined.matches("bar").count(),
        1,
        "prose between codes must not duplicate: {joined}"
    );
    assert_eq!(
        joined.matches("baz").count(),
        1,
        "prose after last code must not duplicate: {joined}"
    );
}

#[test]
fn inline_code_in_blockquote() {
    let styled = markdown_to_styled("> Some `code` here", 80);
    let joined: String = styled
        .iter()
        .map(|e| e.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        joined.contains('`'),
        "inline code backticks should appear in blockquote: {joined}"
    );
    assert!(
        joined.contains("Some"),
        "prose before code should appear in blockquote: {joined}"
    );
}

// ── markdown_to_styled: links ───────────────────────────────────────

#[test]
fn link_renders_url() {
    let styled = markdown_to_styled("Click [here](https://example.com) for more", 80);
    let has_url = styled
        .iter()
        .any(|e| e.text.contains("https://example.com"));
    assert!(has_url, "link URL should appear in output");
}

#[test]
fn link_text_is_colored() {
    let styled = markdown_to_styled("[link text](https://x.com)", 80);
    let cyan_lines: Vec<_> = styled
        .iter()
        .filter(|e| e.color == Color::DarkCyan)
        .collect();
    assert!(!cyan_lines.is_empty(), "link text should be DarkCyan");
}

#[test]
fn link_url_is_dark_grey() {
    let styled = markdown_to_styled("[text](https://x.com)", 80);
    let url_lines: Vec<_> = styled
        .iter()
        .filter(|e| e.color == Color::DarkGrey && e.text.contains('\u{21aa}'))
        .collect();
    assert!(
        !url_lines.is_empty(),
        "link URL should be DarkGrey with arrow"
    );
}

// ── markdown_to_styled: tables ──────────────────────────────────────

#[test]
fn table_renders_borders() {
    let input = "| A | B |\n|---|---|\n| 1 | 2 |\n";
    let styled = markdown_to_styled(input, 80);
    let text: Vec<&str> = styled.iter().map(|e| e.text.as_str()).collect();
    let joined = text.join("");
    assert!(
        joined.contains('\u{250c}'),
        "table should have top-left border"
    );
    assert!(
        joined.contains('\u{2510}'),
        "table should have top-right border"
    );
    assert!(
        joined.contains('\u{2514}'),
        "table should have bottom-left border"
    );
    assert!(
        joined.contains('\u{2502}'),
        "table should have vertical separators"
    );
}

#[test]
fn table_contains_content() {
    let input = "| Name | Age |\n|------|-----|\n| Alice | 30 |\n";
    let styled = markdown_to_styled(input, 80);
    let joined: String = styled
        .iter()
        .map(|e| e.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("Name"),
        "table should contain header 'Name'"
    );
    assert!(
        joined.contains("Alice"),
        "table should contain data 'Alice'"
    );
    assert!(joined.contains("30"), "table should contain data '30'");
}

#[test]
fn table_borders_are_dark_grey() {
    let input = "| X |\n|---|\n| y |\n";
    let styled = markdown_to_styled(input, 80);
    let border_lines: Vec<_> = styled
        .iter()
        .filter(|e| e.color == Color::DarkGrey && e.text.contains('\u{2500}'))
        .collect();
    assert!(!border_lines.is_empty(), "table borders should be DarkGrey");
}

#[test]
fn table_content_is_white() {
    let input = "| X |\n|---|\n| y |\n";
    let styled = markdown_to_styled(input, 80);
    let content_lines: Vec<_> = styled
        .iter()
        .filter(|e| e.color == Color::White && e.text.contains('y'))
        .collect();
    assert!(!content_lines.is_empty(), "table content should be White");
}

#[test]
fn table_blank_skipped() {
    let styled = markdown_to_styled("||\n|--|\n||\n", 80);
    // Fully blank rows are filtered out; only the trailing blank separator remains.
    let content: Vec<_> = styled.iter().filter(|e| e.color == Color::White).collect();
    assert_eq!(content.len(), 1, "only trailing blank separator expected");
    assert!(
        content[0].text.is_empty(),
        "trailing separator should be empty"
    );
    // No DarkGrey border lines because the table had no non-blank rows.
    let borders: Vec<_> = styled
        .iter()
        .filter(|e| e.color == Color::DarkGrey)
        .collect();
    assert!(borders.is_empty(), "no borders for all-blank table");
}

#[test]
fn table_with_inline_code() {
    let input = "| Cmd | Desc |\n|-----|------|\n| `ls` | list |\n";
    let styled = markdown_to_styled(input, 80);
    let joined: String = styled
        .iter()
        .map(|e| e.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("`ls`"),
        "table should contain inline code `ls`"
    );
}

#[test]
fn table_with_alignment() {
    // Headers wider than data so alignment padding is visible.
    let input = "| Left | Center | Right |\n|:-----|:------:|------:|\n| a    | b      | c     |\n";
    let styled = markdown_to_styled(input, 80);
    assert!(!styled.is_empty(), "aligned table should render");

    let joined: String = styled
        .iter()
        .map(|e| e.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let data_row = joined
        .lines()
        .find(|l| l.contains('a') && l.contains('b') && l.contains('c'))
        .expect("data row should exist");
    // Left-aligned: "a" should have trailing spaces before next │
    assert!(
        data_row.contains("│ a  "),
        "left-aligned 'a' should have trailing padding: {data_row}"
    );
    // Right-aligned: "c" should have leading spaces after │
    assert!(
        data_row.contains("  c │"),
        "right-aligned 'c' should have leading padding: {data_row}"
    );
    // Center-aligned: "b" should have spaces on both sides
    assert!(
        data_row.contains("  b  "),
        "center-aligned 'b' should have padding on both sides: {data_row}"
    );
}

#[test]
fn table_respects_max_width() {
    let input = "| Col A | Col B |\n|-------|-------|\n| alpha | beta  |\n";
    for width in [20, 25, 30, 40] {
        let styled = markdown_to_styled(input, width);
        for entry in &styled {
            let w = display_width(&entry.text);
            assert!(
                w <= width,
                "line '{}' has width {} > max_width {}",
                entry.text,
                w,
                width
            );
        }
        assert!(!styled.is_empty(), "table should render at width {}", width);
    }
}

// ── markdown_to_styled: regression ──────────────────────────────────

#[test]
fn empty_input_returns_empty_vec() {
    let styled = markdown_to_styled("", 80);
    assert!(styled.is_empty());
}

#[test]
fn headings_still_work() {
    let styled = markdown_to_styled("# Hello", 80);
    let heading = styled.iter().find(|e| e.color == Color::Cyan);
    assert!(heading.is_some(), "heading should be Cyan");
    assert!(heading.unwrap().text.contains("Hello"));
}

#[test]
fn code_blocks_still_work() {
    let input = "```\nlet x = 1;\n```\n";
    let styled = markdown_to_styled(input, 80);
    let code_lines: Vec<_> = styled
        .iter()
        .filter(|e| e.color == Color::DarkYellow)
        .collect();
    assert!(!code_lines.is_empty(), "code block should be DarkYellow");
}

#[test]
fn lists_still_work() {
    let styled = markdown_to_styled("- item one\n- item two\n", 80);
    let bullets = styled.iter().filter(|e| e.text.contains('\u{2022}'));
    assert_eq!(bullets.count(), 2, "unordered list should have two bullets");
}

#[test]
fn blockquotes_still_work() {
    let styled = markdown_to_styled("> quoted text", 80);
    let quoted = styled.iter().any(|e| e.color == Color::DarkGrey);
    assert!(quoted, "blockquote text should be DarkGrey");
}
