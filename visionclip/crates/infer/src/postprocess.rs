use visionclip_common::ipc::Action;

pub fn sanitize_output(action: &Action, input: &str) -> String {
    let trimmed = input.trim();

    match action {
        Action::ExtractCode => strip_markdown_fences(trimmed),
        Action::CopyText => trimmed.to_string(),
        Action::TranslatePtBr => trimmed.to_string(),
        Action::Explain => trimmed.to_string(),
        Action::SearchWeb => trimmed
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

fn strip_markdown_fences(input: &str) -> String {
    if !(input.starts_with("```") && input.ends_with("```")) {
        return input.to_string();
    }

    let mut lines = input.lines();
    let first = lines.next().unwrap_or_default();
    let last_removed = input.strip_suffix("```").unwrap_or(input);
    let body = last_removed.lines().skip(1).collect::<Vec<_>>().join("\n");

    if first.trim() == "```" {
        body.trim().to_string()
    } else {
        body.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use visionclip_common::ipc::Action;

    #[test]
    fn remove_fences_from_code() {
        let raw = "```rust\nfn main() {}\n```";
        let cleaned = sanitize_output(&Action::ExtractCode, raw);
        assert_eq!(cleaned, "fn main() {}");
    }
}
