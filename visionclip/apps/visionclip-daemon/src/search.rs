use anyhow::{Context, Result};
use reqwest::{header, Client, Url};
use scraper::{Html, Selector};
use std::{collections::HashSet, time::Duration};
use visionclip_common::config::SearchConfig;

const AI_OVERVIEW_LABELS: [&str; 8] = [
    "Visao Geral Criada por IA",
    "Visao geral criada por IA",
    "Visoes Gerais Criadas por IA",
    "AI Overview",
    "Organizado com IA",
    "Resumo com IA",
    "Gerado com IA",
    "AI-generated overview",
];
const AI_OVERVIEW_STOP_MARKERS: [&str; 14] = [
    "As pessoas tambem perguntam",
    "People also ask",
    "Resultados da web",
    "Web results",
    "Imagens",
    "Images",
    "Videos",
    "Forums",
    "Discussoes e forums",
    "Mais resultados",
    "About this result",
    "People also search for",
    "Pesquisas relacionadas",
    "Related searches",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSnippet {
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchEnrichment {
    pub ai_overview: Option<String>,
    pub snippets: Vec<SearchSnippet>,
}

impl SearchEnrichment {
    pub fn clipboard_text(&self, query: &str) -> Option<String> {
        if let Some(overview) = &self.ai_overview {
            let mut sections = vec![
                format!("Pesquisa: {query}"),
                format!("Resumo inicial encontrado na busca:\n{overview}"),
            ];

            if !self.snippets.is_empty() {
                sections.push(format!(
                    "Fontes iniciais:\n{}",
                    self.snippets
                        .iter()
                        .take(3)
                        .enumerate()
                        .map(|(index, item)| format_snippet_entry(index, item))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }

            return Some(sections.join("\n\n"));
        }

        if self.snippets.is_empty() {
            return None;
        }

        Some(format!(
            "Pesquisa: {query}\n\nSinais iniciais encontrados na busca:\n{}",
            self.snippets
                .iter()
                .take(3)
                .enumerate()
                .map(|(index, item)| format_snippet_entry(index, item))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }

    pub fn spoken_text(&self, query: &str) -> Option<String> {
        if let Some(overview) = &self.ai_overview {
            return Some(overview.clone());
        }

        if self.snippets.is_empty() {
            return Some(format!(
                "Pesquisa aberta no navegador para aprofundar o tema: {query}"
            ));
        }

        let combined = self
            .snippets
            .iter()
            .take(2)
            .map(|item| {
                if item.snippet.is_empty() {
                    item.title.clone()
                } else {
                    item.snippet.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        Some(combined)
    }
}

#[derive(Debug, Clone)]
pub struct GoogleSearchClient {
    client: Client,
    config: SearchConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchFetchMode {
    Standard,
    WebResults,
}

impl GoogleSearchClient {
    pub fn new(config: SearchConfig) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36",
            ),
        );
        headers.insert(
            header::ACCEPT_LANGUAGE,
            header::HeaderValue::from_static("pt-BR,pt;q=0.9,en;q=0.8"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_millis(config.request_timeout_ms))
            .build()
            .context("failed to build Google search client")?;

        Ok(Self { client, config })
    }

    pub async fn search(&self, query: &str) -> Result<SearchEnrichment> {
        let primary = match self.fetch_and_parse(query, SearchFetchMode::Standard).await {
            Ok(enrichment) => enrichment,
            Err(primary_error) => {
                return self
                    .fetch_and_parse(query, SearchFetchMode::WebResults)
                    .await
                    .with_context(|| {
                        format!(
                            "failed to fetch Google search page in standard and web-results modes; primary error: {primary_error}"
                        )
                    });
            }
        };

        let fallback = self
            .fetch_and_parse(query, SearchFetchMode::WebResults)
            .await
            .unwrap_or_else(|_| SearchEnrichment {
                ai_overview: None,
                snippets: Vec::new(),
            });

        let merged = merge_enrichment(primary, fallback, self.config.max_results);
        if merged.ai_overview.is_some() || !merged.snippets.is_empty() {
            return Ok(merged);
        }

        Ok(merged)
    }

    async fn fetch_and_parse(
        &self,
        query: &str,
        mode: SearchFetchMode,
    ) -> Result<SearchEnrichment> {
        let url = build_fetch_url(&self.config.base_url, query, self.config.max_results, mode)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("failed to fetch Google search page")?
            .error_for_status()
            .context("Google search returned an error status")?;
        let html = response
            .text()
            .await
            .context("failed to read Google search response body")?;

        Ok(parse_google_search_html(&html, self.config.max_results))
    }
}

fn build_fetch_url(
    base_url: &str,
    query: &str,
    max_results: usize,
    mode: SearchFetchMode,
) -> Result<Url> {
    let mut url =
        Url::parse(base_url).with_context(|| format!("invalid search base URL `{base_url}`"))?;
    {
        let mut query_pairs = url.query_pairs_mut();
        query_pairs
            .append_pair("hl", "pt-BR")
            .append_pair("gl", "br")
            .append_pair("num", &max_results.to_string())
            .append_pair("q", query.trim());

        if mode == SearchFetchMode::WebResults {
            query_pairs.append_pair("udm", "14");
        }
    }

    Ok(url)
}

fn parse_google_search_html(html: &str, max_results: usize) -> SearchEnrichment {
    let document = Html::parse_document(html);
    SearchEnrichment {
        ai_overview: extract_ai_overview(&document),
        snippets: extract_search_results(&document, max_results),
    }
}

fn extract_ai_overview(document: &Html) -> Option<String> {
    let lines = visible_text_lines(document);

    for (index, line) in lines.iter().enumerate() {
        if !is_ai_overview_label(line) {
            continue;
        }

        let mut collected = Vec::new();
        let mut chars = 0_usize;

        for candidate in lines.iter().skip(index + 1) {
            if candidate.is_empty() || is_ai_overview_label(candidate) {
                continue;
            }
            if is_ai_overview_stop_marker(candidate) {
                break;
            }

            chars += candidate.chars().count();
            collected.push(candidate.clone());

            if collected.len() >= 4 || chars >= 650 {
                break;
            }
        }

        let merged = collected.join(" ");
        if !merged.is_empty() {
            return Some(merged);
        }
    }

    None
}

fn extract_search_results(document: &Html, max_results: usize) -> Vec<SearchSnippet> {
    let containers = selector_list(&[
        "div.g",
        "div.MjjYud",
        "div.Gx5Zad",
        "div.N54PNb",
        "div.tF2Cxc",
        "div[data-snc]",
    ]);
    let title_selectors = selector_list(&["h3", "div[role='heading']"]);
    let link_selector = Selector::parse("a[href]").expect("valid anchor selector");
    let snippet_selectors = selector_list(&[
        "div.VwiC3b",
        "span.aCOpRe",
        "div[data-sncf]",
        "div.s3v9rd",
        "div.ITZIwc",
        "div.yXK7lf",
    ]);

    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for container_selector in &containers {
        for container in document.select(container_selector) {
            let mut title = title_selectors
                .iter()
                .find_map(|selector| first_text_in(&container, selector))
                .unwrap_or_default();
            let Some(link) = container
                .select(&link_selector)
                .find_map(|element| normalize_search_href(element.value().attr("href")?))
            else {
                continue;
            };
            let mut snippet = snippet_selectors
                .iter()
                .find_map(|selector| first_text_in(&container, selector))
                .unwrap_or_default();

            if title.is_empty() {
                title = container
                    .select(&link_selector)
                    .map(|element| normalize_text(&element.text().collect::<Vec<_>>().join(" ")))
                    .find(|value| !value.is_empty())
                    .unwrap_or_else(|| domain_from_url(&link));
            }

            if snippet.is_empty() {
                snippet = normalize_text(&container.text().collect::<Vec<_>>().join(" "));
                if snippet == title {
                    snippet.clear();
                } else if !title.is_empty() {
                    snippet = snippet.replacen(&title, "", 1);
                    snippet = normalize_text(&snippet);
                }
            }

            if title.is_empty() && snippet.is_empty() {
                continue;
            }

            let key = format!("{title}::{link}");
            if !seen.insert(key) {
                continue;
            }

            let domain = domain_from_url(&link);

            items.push(SearchSnippet {
                title,
                url: link,
                domain,
                snippet,
            });

            if items.len() >= max_results {
                return items;
            }
        }
    }

    items
}

fn merge_enrichment(
    primary: SearchEnrichment,
    fallback: SearchEnrichment,
    max_results: usize,
) -> SearchEnrichment {
    let mut seen = HashSet::new();
    let mut snippets = Vec::new();

    for item in primary.snippets.into_iter().chain(fallback.snippets) {
        let key = format!("{}::{}", item.title, item.url);
        if !seen.insert(key) {
            continue;
        }
        snippets.push(item);
        if snippets.len() >= max_results {
            break;
        }
    }

    SearchEnrichment {
        ai_overview: primary.ai_overview.or(fallback.ai_overview),
        snippets,
    }
}

fn format_snippet_entry(index: usize, item: &SearchSnippet) -> String {
    if item.snippet.is_empty() {
        format!("{}. {} ({})", index + 1, item.title, item.domain)
    } else {
        format!(
            "{}. {} ({})\n{}",
            index + 1,
            item.title,
            item.domain,
            item.snippet
        )
    }
}

fn visible_text_lines(document: &Html) -> Vec<String> {
    document
        .root_element()
        .text()
        .map(normalize_text)
        .filter(|line| !line.is_empty())
        .collect()
}

fn first_text_in(container: &scraper::ElementRef<'_>, selector: &Selector) -> Option<String> {
    container
        .select(selector)
        .map(|element| normalize_text(&element.text().collect::<Vec<_>>().join(" ")))
        .find(|text| !text.is_empty())
}

fn normalize_search_href(href: &str) -> Option<String> {
    if href.starts_with("/url?") {
        let absolute = format!("https://www.google.com{href}");
        let url = Url::parse(&absolute).ok()?;
        let target = url.query_pairs().find_map(|(name, value)| {
            if name == "q" {
                Some(value.into_owned())
            } else {
                None
            }
        })?;
        if target.starts_with("http://") || target.starts_with("https://") {
            return Some(target);
        }
        return None;
    }

    if href.starts_with("http://") || href.starts_with("https://") {
        let url = Url::parse(href).ok()?;
        let host = url.host_str().unwrap_or_default();
        if host.contains("google.") {
            return None;
        }
        return Some(href.to_string());
    }

    None
}

fn domain_from_url(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .unwrap_or_default()
}

fn selector_list(selectors: &[&str]) -> Vec<Selector> {
    selectors
        .iter()
        .map(|selector| Selector::parse(selector).expect("valid CSS selector"))
        .collect()
}

fn normalize_text(input: &str) -> String {
    let ascii = input
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    ascii.trim().to_string()
}

fn is_ai_overview_label(line: &str) -> bool {
    let normalized = ascii_fold(line);
    AI_OVERVIEW_LABELS
        .iter()
        .any(|label| normalized.contains(&ascii_fold(label)))
}

fn is_ai_overview_stop_marker(line: &str) -> bool {
    let normalized = ascii_fold(line);
    AI_OVERVIEW_STOP_MARKERS
        .iter()
        .any(|marker| normalized.contains(&ascii_fold(marker)))
}

fn ascii_fold(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            'á' | 'à' | 'ã' | 'â' | 'ä' | 'Á' | 'À' | 'Ã' | 'Â' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'õ' | 'ô' | 'ö' | 'Ó' | 'Ò' | 'Õ' | 'Ô' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            other => other.to_ascii_lowercase(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_google_html_extracts_ai_overview_and_results() {
        let html = r#"
            <html><body>
              <div>Visao Geral Criada por IA</div>
              <div>O daemon coordena clipboard, TTS e inferencia local.</div>
              <div>Ele pode abrir a pesquisa para aprofundamento.</div>
              <div>As pessoas tambem perguntam</div>
              <div class="g">
                <a href="/url?q=https://example.com/docs&sa=U"><h3>VisionClip Docs</h3></a>
                <div class="VwiC3b">Guia de configuracao do daemon e do Piper.</div>
              </div>
              <div class="g">
                <a href="/url?q=https://example.org/forum&sa=U"><h3>Forum Thread</h3></a>
                <span class="aCOpRe">Discussao sobre captura via portal e fallback.</span>
              </div>
            </body></html>
        "#;

        let enrichment = parse_google_search_html(html, 3);
        assert_eq!(
            enrichment.ai_overview,
            Some("O daemon coordena clipboard, TTS e inferencia local. Ele pode abrir a pesquisa para aprofundamento.".to_string())
        );
        assert_eq!(enrichment.snippets.len(), 2);
        assert_eq!(enrichment.snippets[0].title, "VisionClip Docs");
        assert_eq!(enrichment.snippets[0].domain, "example.com");
    }

    #[test]
    fn normalize_search_href_extracts_google_redirect_target() {
        let href = "/url?q=https://example.com/path&sa=U&ved=2ah";
        assert_eq!(
            normalize_search_href(href),
            Some("https://example.com/path".to_string())
        );
    }

    #[test]
    fn build_fetch_url_adds_web_results_mode_when_requested() {
        let url = build_fetch_url(
            "https://www.google.com/search",
            "visionclip daemon",
            3,
            SearchFetchMode::WebResults,
        )
        .unwrap();

        assert!(url.as_str().contains("udm=14"));
        assert!(url.as_str().contains("q=visionclip+daemon"));
    }

    #[test]
    fn clipboard_text_prioritizes_overview() {
        let enrichment = SearchEnrichment {
            ai_overview: Some("Resumo objetivo da busca.".into()),
            snippets: vec![SearchSnippet {
                title: "Fonte 1".into(),
                url: "https://example.com".into(),
                domain: "example.com".into(),
                snippet: "Detalhe complementar.".into(),
            }],
        };

        let text = enrichment
            .clipboard_text("erro visionclip portal")
            .expect("clipboard summary");
        assert!(text.contains("Resumo inicial encontrado na busca"));
        assert!(text.contains("Fonte 1"));
    }

    #[test]
    fn merge_enrichment_preserves_primary_and_appends_fallback_snippets() {
        let primary = SearchEnrichment {
            ai_overview: Some("Resumo principal".into()),
            snippets: vec![SearchSnippet {
                title: "Docs".into(),
                url: "https://example.com/docs".into(),
                domain: "example.com".into(),
                snippet: "Guia principal.".into(),
            }],
        };
        let fallback = SearchEnrichment {
            ai_overview: None,
            snippets: vec![SearchSnippet {
                title: "Forum".into(),
                url: "https://example.org/forum".into(),
                domain: "example.org".into(),
                snippet: "Discussao complementar.".into(),
            }],
        };

        let merged = merge_enrichment(primary, fallback, 3);
        assert_eq!(merged.ai_overview, Some("Resumo principal".into()));
        assert_eq!(merged.snippets.len(), 2);
        assert_eq!(merged.snippets[1].title, "Forum");
    }
}
