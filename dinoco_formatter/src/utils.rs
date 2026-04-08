pub fn get_capped_newlines(current_end_line: usize, next_start_line: usize, next_is_comment: bool) -> String {
    if next_start_line <= current_end_line && next_is_comment {
        return " ".into();
    }

    let diff = next_start_line.saturating_sub(current_end_line);

    match diff {
        0 | 1 => "\n".into(),
        _ => "\n\n".into(),
    }
}

pub fn format_newlines(count: usize, is_inline: bool) -> String {
    if count == 0 {
        return if is_inline { " ".into() } else { "\n".into() };
    }

    "\n".repeat(count.min(2))
}
