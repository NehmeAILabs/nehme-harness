pub fn prev_char_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx.saturating_sub(1);
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub fn next_char_boundary(s: &str, idx: usize) -> usize {
    let len = s.len();
    let mut i = (idx + 1).min(len);
    while i < len && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

pub fn cursor_to_line_col(buffer: &str, cursor: usize) -> (usize, usize) {
    let mut line = 0usize;
    let mut col = 0usize;
    for (i, ch) in buffer.char_indices() {
        if i >= cursor {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub fn line_col_to_cursor(buffer: &str, target_line: usize, target_col: usize) -> usize {
    let mut line = 0usize;
    let mut col = 0usize;
    for (i, ch) in buffer.char_indices() {
        if line == target_line && col == target_col {
            return i;
        }
        if ch == '\n' {
            if line == target_line {
                return i;
            }
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    buffer.len()
}

pub fn count_lines(buffer: &str) -> usize {
    buffer.chars().filter(|&c| c == '\n').count() + 1
}

pub fn line_start(buffer: &str, cursor: usize) -> usize {
    let (line, _) = cursor_to_line_col(buffer, cursor);
    line_col_to_cursor(buffer, line, 0)
}

pub fn line_end(buffer: &str, cursor: usize) -> usize {
    let start = line_start(buffer, cursor);
    buffer[start..]
        .find('\n')
        .map(|pos| start + pos)
        .unwrap_or(buffer.len())
}
