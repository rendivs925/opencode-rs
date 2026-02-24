#[napi]
pub struct TruncationResult {
    pub text: String,
    pub lines: i32,
    pub bytes: i32,
    pub truncated: bool,
}

#[napi]
pub fn truncate(
    text: String,
    max_lines: Option<i32>,
    max_bytes: Option<i32>,
    direction: String,
) -> TruncationResult {
    let max_lines = max_lines.unwrap_or(i32::MAX);
    let max_bytes = max_bytes.unwrap_or(i32::MAX);

    let lines: Vec<&str> = text.lines().collect();
    let total_bytes = text.len();

    let (selected_lines, truncated) = if direction == "tail" {
        let start = if lines.len() > max_lines as usize {
            lines.len() - max_lines as usize
        } else {
            0
        };
        let selected: Vec<&str> = lines[start..].to_vec();
        (selected, lines.len() > max_lines as usize)
    } else {
        let end = if lines.len() > max_lines as usize {
            max_lines as usize
        } else {
            lines.len()
        };
        let selected: Vec<&str> = lines[..end].to_vec();
        (selected, lines.len() > max_lines as usize)
    };

    let truncated_text = selected_lines.join("\n");
    let truncated_bytes = truncated_text.len();

    let final_text = if truncated_bytes > max_bytes as usize {
        String::from_utf8_lossy(&truncated_text.as_bytes()[..max_bytes as usize]).to_string()
    } else {
        truncated_text
    };

    let result_lines = final_text.lines().count();
    let result_bytes = final_text.len();

    TruncationResult {
        text: final_text,
        lines: result_lines as i32,
        bytes: result_bytes as i32,
        truncated: truncated || total_bytes > max_bytes as usize,
    }
}

#[napi]
pub fn truncate_lines(text: String, max_lines: i32) -> TruncationResult {
    truncate(text, Some(max_lines), None, "head".to_string())
}

#[napi]
pub fn truncate_bytes(text: String, max_bytes: i32) -> TruncationResult {
    truncate(text, None, Some(max_bytes), "head".to_string())
}

#[napi]
pub fn truncate_tail(text: String, max_lines: i32) -> TruncationResult {
    truncate(text, Some(max_lines), None, "tail".to_string())
}
