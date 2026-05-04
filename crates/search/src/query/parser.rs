#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub original: String,
    pub terms: Vec<String>,
    pub phrases: Vec<String>,
    pub filters: Vec<QueryFilter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryFilter {
    Kind(String),
    Extension(String),
    Path(String),
    Modified(String),
    Source(String),
    Size(String),
}

pub fn parse_query(input: &str) -> SearchQuery {
    let tokens = tokenize(input);
    let mut terms = Vec::new();
    let mut phrases = Vec::new();
    let mut filters = Vec::new();

    for token in tokens {
        if token.quoted {
            let value = token.value.trim();
            if !value.is_empty() {
                phrases.push(value.to_string());
                terms.extend(split_terms(value));
            }
            continue;
        }

        if let Some(filter) = parse_filter(&token.value) {
            filters.push(filter);
            continue;
        }

        if is_boolean_operator(&token.value) {
            continue;
        }

        terms.extend(split_terms(&token.value));
    }

    SearchQuery {
        original: input.trim().to_string(),
        terms,
        phrases,
        filters,
    }
}

#[derive(Debug, Clone)]
struct Token {
    value: String,
    quoted: bool,
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut current_quoted = false;

    for ch in input.chars() {
        match ch {
            '"' => {
                if quoted {
                    tokens.push(Token {
                        value: current.trim().to_string(),
                        quoted: true,
                    });
                    current.clear();
                    quoted = false;
                    current_quoted = false;
                } else {
                    if !current.trim().is_empty() {
                        tokens.push(Token {
                            value: current.trim().to_string(),
                            quoted: current_quoted,
                        });
                    }
                    current.clear();
                    quoted = true;
                    current_quoted = true;
                }
            }
            ch if ch.is_whitespace() && !quoted => {
                if !current.trim().is_empty() {
                    tokens.push(Token {
                        value: current.trim().to_string(),
                        quoted: current_quoted,
                    });
                    current.clear();
                    current_quoted = false;
                }
            }
            other => current.push(other),
        }
    }

    if !current.trim().is_empty() {
        tokens.push(Token {
            value: current.trim().to_string(),
            quoted: current_quoted,
        });
    }

    tokens
}

fn parse_filter(token: &str) -> Option<QueryFilter> {
    let (key, value) = token.split_once(':')?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    match key.trim().to_ascii_lowercase().as_str() {
        "kind" => Some(QueryFilter::Kind(value.to_ascii_lowercase())),
        "ext" => Some(QueryFilter::Extension(
            value.trim_start_matches('.').to_ascii_lowercase(),
        )),
        "path" => Some(QueryFilter::Path(value.to_ascii_lowercase())),
        "modified" => Some(QueryFilter::Modified(value.to_ascii_lowercase())),
        "source" => Some(QueryFilter::Source(value.to_ascii_lowercase())),
        "size" => Some(QueryFilter::Size(value.to_ascii_lowercase())),
        _ => None,
    }
}

fn is_boolean_operator(token: &str) -> bool {
    matches!(token.to_ascii_uppercase().as_str(), "AND" | "OR" | "NOT")
}

fn split_terms(value: &str) -> Vec<String> {
    value
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '.' || ch == '_' || ch == '-'))
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_filters_terms_and_phrases() {
        let parsed = parse_query(r#""auth middleware" kind:code ext:rs path:src foo AND bar"#);

        assert_eq!(parsed.phrases, vec!["auth middleware"]);
        assert!(parsed.terms.contains(&"auth".to_string()));
        assert!(parsed.terms.contains(&"middleware".to_string()));
        assert!(parsed.terms.contains(&"foo".to_string()));
        assert!(parsed.terms.contains(&"bar".to_string()));
        assert_eq!(
            parsed.filters,
            vec![
                QueryFilter::Kind("code".to_string()),
                QueryFilter::Extension("rs".to_string()),
                QueryFilter::Path("src".to_string())
            ]
        );
    }
}
