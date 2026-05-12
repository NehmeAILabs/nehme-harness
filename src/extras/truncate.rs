/// Truncate `s` to at most `max` bytes on a UTF-8 char boundary, appending
/// `marker` so callers know content was capped. Plain `String::truncate` panics
/// mid-character (e.g. on CJK); this walks back to the nearest boundary.
pub(crate) fn truncate_cjk(s: &str, max: usize, marker: &str) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut cut = max;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut out = s[..cut].to_string();
    out.push_str(marker);
    out
}

/// Keep the first `max` lines of `s`. Returns `(head, total_lines)` where
/// `total_lines` is the original line count. If `total_lines <= max`, `head`
/// equals `s` (no truncation needed). Callers should append a tool-specific
/// recovery hint when truncation occurred.
pub(crate) fn head_lines(s: &str, max: usize) -> (String, usize) {
    let total = s.lines().count();
    if total <= max {
        return (s.to_string(), total);
    }
    let head: String = s.lines().take(max).collect::<Vec<_>>().join("\n");
    (head, total)
}
