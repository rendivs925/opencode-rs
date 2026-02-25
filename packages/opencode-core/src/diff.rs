use napi::Result;

#[napi(object)]
#[derive(Clone)]
pub struct DiffLine {
    pub old_index: Option<u32>,
    pub new_index: Option<u32>,
    pub content: String,
    pub line_type: String,
}

#[napi]
pub fn create_two_files_patch(
    old_path: String,
    new_path: String,
    old_content: String,
    new_content: String,
) -> String {
    let mut out = vec![
        format!("--- {old_path}"),
        format!("+++ {new_path}"),
        format!(
            "@@ -1,{} +1,{} @@",
            old_content.lines().count(),
            new_content.lines().count()
        ),
    ];
    for item in diff_lines(old_content, new_content) {
        let prefix = match item.line_type.as_str() {
            "added" => "+",
            "removed" => "-",
            _ => " ",
        };
        out.push(format!("{prefix}{}", item.content));
    }
    out.join("\n")
}

#[napi]
pub fn diff_lines(old: String, new: String) -> Vec<DiffLine> {
    let old = old.lines().collect::<Vec<_>>();
    let new = new.lines().collect::<Vec<_>>();
    let mut out = vec![];
    let mut i = 0usize;
    let mut j = 0usize;

    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            out.push(DiffLine {
                old_index: Some(i as u32),
                new_index: Some(j as u32),
                content: old[i].to_string(),
                line_type: "context".to_string(),
            });
            i += 1;
            j += 1;
            continue;
        }

        if i < old.len() && (j >= new.len() || old.get(i + 1) == new.get(j)) {
            out.push(DiffLine {
                old_index: Some(i as u32),
                new_index: None,
                content: old[i].to_string(),
                line_type: "removed".to_string(),
            });
            i += 1;
            continue;
        }

        if j < new.len() && (i >= old.len() || old.get(i) == new.get(j + 1)) {
            out.push(DiffLine {
                old_index: None,
                new_index: Some(j as u32),
                content: new[j].to_string(),
                line_type: "added".to_string(),
            });
            j += 1;
            continue;
        }

        if i < old.len() {
            out.push(DiffLine {
                old_index: Some(i as u32),
                new_index: None,
                content: old[i].to_string(),
                line_type: "removed".to_string(),
            });
            i += 1;
        }
        if j < new.len() {
            out.push(DiffLine {
                old_index: None,
                new_index: Some(j as u32),
                content: new[j].to_string(),
                line_type: "added".to_string(),
            });
            j += 1;
        }
    }

    out
}

#[napi]
pub fn apply_patch(original: String, patch: String) -> Result<String> {
    let mut src = original.lines().map(ToString::to_string).collect::<Vec<_>>();
    let mut idx = 0usize;

    for line in patch.lines() {
        if line.starts_with("--- ") || line.starts_with("+++ ") || line.starts_with("@@ ") {
            continue;
        }
        if let Some(rest) = line.strip_prefix(' ') {
            if src.get(idx).map(|item| item.as_str()) != Some(rest) {
                return Err(napi::Error::from_reason("patch context mismatch".to_string()));
            }
            idx += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix('-') {
            if src.get(idx).map(|item| item.as_str()) != Some(rest) {
                return Err(napi::Error::from_reason("patch delete mismatch".to_string()));
            }
            src.remove(idx);
            continue;
        }
        if let Some(rest) = line.strip_prefix('+') {
            src.insert(idx, rest.to_string());
            idx += 1;
        }
    }

    Ok(src.join("\n"))
}
