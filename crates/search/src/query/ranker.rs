use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryShape {
    Short,
    Natural,
    Code,
}

pub fn classify_query(terms: &[String], original: &str) -> QueryShape {
    let has_code_hint = terms.iter().any(|term| {
        matches!(
            term.as_str(),
            "fn" | "function" | "struct" | "impl" | "src" | "auth" | "middleware"
        ) || term.ends_with(".rs")
            || term.ends_with(".ts")
            || term.ends_with(".js")
            || term.ends_with(".py")
    });
    if has_code_hint {
        return QueryShape::Code;
    }
    if terms.len() <= 2 && original.chars().count() <= 32 {
        QueryShape::Short
    } else {
        QueryShape::Natural
    }
}

pub fn score_name_path_hit(path: &Path, title: &str, terms: &[String], original: &str) -> f32 {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let title = title.to_ascii_lowercase();
    let path_text = path.display().to_string().to_ascii_lowercase();
    let query = original.trim().to_ascii_lowercase();
    if query.is_empty() || terms.is_empty() {
        return 0.0;
    }

    let mut score = 0.0_f32;
    if file_name == query {
        score += 1000.0;
    }
    if title == query {
        score += 900.0;
    }
    if file_name.contains(&query) {
        score += 380.0;
    }
    if title.contains(&query) {
        score += 320.0;
    }
    if path_text.contains(&query) {
        score += 120.0;
    }

    for term in terms {
        if file_name.contains(term) {
            score += 90.0;
        }
        if title.contains(term) {
            score += 70.0;
        }
        if path_text.contains(term) {
            score += 24.0;
        }
    }

    match classify_query(terms, original) {
        QueryShape::Short => score * 1.15,
        QueryShape::Code => {
            if path_text.contains("/src/") || path_text.contains("\\src\\") {
                score += 80.0;
            }
            score
        }
        QueryShape::Natural => score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_filename_scores_above_path_only_match() {
        let exact = score_name_path_hit(
            Path::new("/tmp/project/docker-compose.yml"),
            "docker-compose",
            &["docker-compose.yml".to_string()],
            "docker-compose.yml",
        );
        let path_only = score_name_path_hit(
            Path::new("/tmp/docker/project/config.yml"),
            "config",
            &["docker".to_string()],
            "docker",
        );

        assert!(exact > path_only);
    }
}
