use anyhow::{Context, Result};
use reqwest::{header, Client, Url};
use scraper::{Html, Selector};
use serde_json::Value;
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
const RELATED_QUESTIONS_LABELS: [&str; 4] = [
    "As pessoas tambem perguntam",
    "People also ask",
    "Outras perguntas",
    "Perguntas relacionadas",
];
const RELATED_SEARCHES_LABELS: [&str; 4] = [
    "Pesquisas relacionadas",
    "Related searches",
    "People also search for",
    "Mais pesquisas",
];
const GOOGLE_CHALLENGE_MARKERS: [&str; 6] = [
    "If you're having trouble accessing Google Search",
    "Clique aqui se o redirecionamento nao iniciar",
    "/httpservice/retry/enablejs",
    "id=\"yvlrue\"",
    "cad=sg_trbl",
    "SG_SS=",
];
const GOOGLE_CHALLENGE_ERROR_MARKERS: [&str; 3] = [
    "google search returned a challenge page",
    "blocked local scraping",
    "google bloqueou a coleta local",
];
const DUCKDUCKGO_CHALLENGE_MARKERS: [&str; 4] = [
    "Unfortunately, bots use DuckDuckGo too",
    "anomaly-modal",
    "challenge-form",
    "/anomaly.js",
];
const WIKIPEDIA_LANGUAGES: [&str; 2] = ["pt", "en"];

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
    pub related_questions: Vec<String>,
    pub related_searches: Vec<String>,
}

impl SearchEnrichment {
    fn has_useful_content(&self) -> bool {
        self.ai_overview.is_some()
            || !self.snippets.is_empty()
            || !self.related_questions.is_empty()
            || !self.related_searches.is_empty()
    }

    pub fn clipboard_text(&self, query: &str) -> Option<String> {
        let summary = self.summary_text();
        let supporting_points = self.supporting_points();
        if summary.is_none()
            && self.snippets.is_empty()
            && self.related_questions.is_empty()
            && self.related_searches.is_empty()
        {
            return None;
        }

        let mut sections = vec![format!("Pesquisa: {query}")];

        if let Some(summary) = summary {
            sections.push(format!("Leitura inicial:\n{summary}"));
        }

        if self.ai_overview.is_some() && !supporting_points.is_empty() {
            sections.push(format!(
                "Pistas iniciais:\n{}",
                supporting_points
                    .iter()
                    .take(3)
                    .enumerate()
                    .map(|(index, item)| format!("{}. {}", index + 1, item))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        if !self.related_questions.is_empty() {
            sections.push(format!(
                "Perguntas para aprofundar:\n{}",
                self.related_questions
                    .iter()
                    .take(3)
                    .enumerate()
                    .map(|(index, item)| format!("{}. {}", index + 1, item))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        if !self.related_searches.is_empty() {
            sections.push(format!(
                "Buscas relacionadas:\n{}",
                self.related_searches
                    .iter()
                    .take(4)
                    .enumerate()
                    .map(|(index, item)| format!("{}. {}", index + 1, item))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

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

        Some(sections.join("\n\n"))
    }

    pub fn spoken_text(&self, query: &str) -> Option<String> {
        if let Some(summary) = self.summary_text() {
            let summary = truncate_chars(&summary, 280);
            if let Some(follow_up) = self
                .related_questions
                .first()
                .map(String::as_str)
                .or_else(|| self.related_searches.first().map(String::as_str))
            {
                return Some(format!(
                    "{} Para aprofundar, considere: {}",
                    summary,
                    truncate_chars(follow_up, 96)
                ));
            }
            return Some(summary);
        }

        if !self.related_questions.is_empty() {
            return Some(format!(
                "Pesquisa aberta. Uma pergunta útil para aprofundar é: {}",
                truncate_chars(&self.related_questions[0], 120)
            ));
        }

        if !self.related_searches.is_empty() {
            return Some(format!(
                "Pesquisa aberta. Um bom próximo termo é: {}",
                truncate_chars(&self.related_searches[0], 120)
            ));
        }

        if self.snippets.is_empty() {
            return Some(format!(
                "Pesquisa aberta no navegador para aprofundar o tema: {query}"
            ));
        }

        let combined = self
            .supporting_points()
            .iter()
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");

        Some(truncate_chars(&combined, 280))
    }

    fn summary_text(&self) -> Option<String> {
        if let Some(overview) = &self.ai_overview {
            return Some(truncate_chars(overview, 420));
        }

        let points = self.supporting_points();
        if points.is_empty() {
            None
        } else if points.len() == 1 {
            Some(points[0].clone())
        } else {
            Some(
                points
                    .iter()
                    .take(2)
                    .map(|point| ensure_sentence(point))
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        }
    }

    fn supporting_points(&self) -> Vec<String> {
        let mut points = Vec::new();
        let mut seen = HashSet::new();

        for item in self.snippets.iter().take(3) {
            let Some(point) = supporting_point_from_snippet(item) else {
                continue;
            };
            let folded = ascii_fold(&point);
            if !seen.insert(folded) {
                continue;
            }
            points.push(point);
        }

        points
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
        let mut failure_notes = Vec::new();
        let google_result = self.search_google(query).await;
        match google_result {
            Ok(enrichment) if enrichment.has_useful_content() => return Ok(enrichment),
            Ok(enrichment) if !self.config.fallback_enabled => return Ok(enrichment),
            Ok(_) => {}
            Err(google_error) if !self.config.fallback_enabled => return Err(google_error),
            Err(google_error) => {
                failure_notes.push(format!("Google local search failed: {google_error}"));
            }
        }

        match self.fetch_duckduckgo_and_parse(query).await {
            Ok(enrichment) if enrichment.has_useful_content() => return Ok(enrichment),
            Ok(_) => {
                failure_notes
                    .push("Google and DuckDuckGo returned no useful local search data".to_string());
            }
            Err(error) => {
                failure_notes.push(format!("DuckDuckGo fallback search failed: {error}"));
            }
        }

        match self.fetch_wikipedia_and_parse(query).await {
            Ok(enrichment) if enrichment.has_useful_content() => Ok(enrichment),
            Ok(enrichment) => Ok(enrichment),
            Err(error) => {
                if failure_notes.is_empty() {
                    Err(error)
                } else {
                    Err(error.context(format!(
                        "Wikipedia knowledge fallback failed after previous search failures: {}",
                        failure_notes.join("; ")
                    )))
                }
            }
        }
    }

    async fn search_google(&self, query: &str) -> Result<SearchEnrichment> {
        let primary = match self
            .fetch_google_and_parse(query, SearchFetchMode::Standard)
            .await
        {
            Ok(enrichment) => enrichment,
            Err(primary_error) => {
                return self
                    .fetch_google_and_parse(query, SearchFetchMode::WebResults)
                    .await
                    .with_context(|| {
                        format!(
                            "failed to fetch Google search page in standard and web-results modes; primary error: {primary_error}"
                        )
                    });
            }
        };

        let fallback = self
            .fetch_google_and_parse(query, SearchFetchMode::WebResults)
            .await
            .unwrap_or_else(|_| SearchEnrichment {
                ai_overview: None,
                snippets: Vec::new(),
                related_questions: Vec::new(),
                related_searches: Vec::new(),
            });

        let merged = merge_enrichment(primary, fallback, self.config.max_results);
        if merged.ai_overview.is_some() || !merged.snippets.is_empty() {
            return Ok(merged);
        }

        Ok(merged)
    }

    async fn fetch_google_and_parse(
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

        if is_google_challenge_page(&html) {
            anyhow::bail!("google search returned a challenge page and blocked local scraping");
        }

        Ok(parse_google_search_html(&html, self.config.max_results))
    }

    async fn fetch_duckduckgo_and_parse(&self, query: &str) -> Result<SearchEnrichment> {
        let url = build_duckduckgo_url(&self.config.fallback_base_url, query)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("failed to fetch fallback search page")?
            .error_for_status()
            .context("fallback search returned an error status")?;
        let html = response
            .text()
            .await
            .context("failed to read fallback search response body")?;
        if is_duckduckgo_challenge_page(&html) {
            anyhow::bail!("DuckDuckGo returned a challenge page and blocked local scraping");
        }

        Ok(parse_duckduckgo_search_html(&html, self.config.max_results))
    }

    async fn fetch_wikipedia_and_parse(&self, query: &str) -> Result<SearchEnrichment> {
        for candidate in wikipedia_search_candidates(query) {
            for lang in WIKIPEDIA_LANGUAGES {
                match self.fetch_wikipedia_summary(&candidate, lang).await {
                    Ok(Some(snippet)) => {
                        return Ok(SearchEnrichment {
                            ai_overview: None,
                            snippets: vec![snippet],
                            related_questions: Vec::new(),
                            related_searches: Vec::new(),
                        });
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::debug!(
                            ?error,
                            candidate,
                            lang,
                            "Wikipedia fallback candidate failed"
                        );
                    }
                }
            }
        }

        Ok(SearchEnrichment {
            ai_overview: None,
            snippets: Vec::new(),
            related_questions: Vec::new(),
            related_searches: Vec::new(),
        })
    }

    async fn fetch_wikipedia_summary(
        &self,
        query: &str,
        lang: &str,
    ) -> Result<Option<SearchSnippet>> {
        let title = self
            .fetch_wikipedia_title(query, lang)
            .await?
            .filter(|title| !title.trim().is_empty());
        let Some(title) = title else {
            return Ok(None);
        };

        let url = build_wikipedia_summary_url(lang, &title)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("failed to fetch Wikipedia summary for `{title}`"))?
            .error_for_status()
            .with_context(|| format!("Wikipedia summary returned an error for `{title}`"))?;
        let value = response
            .json::<Value>()
            .await
            .with_context(|| format!("failed to parse Wikipedia summary for `{title}`"))?;

        Ok(parse_wikipedia_summary(&value, lang))
    }

    async fn fetch_wikipedia_title(&self, query: &str, lang: &str) -> Result<Option<String>> {
        let url = build_wikipedia_opensearch_url(lang, query)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("failed to fetch Wikipedia opensearch for `{query}`"))?
            .error_for_status()
            .with_context(|| format!("Wikipedia opensearch returned an error for `{query}`"))?;
        let value = response
            .json::<Value>()
            .await
            .with_context(|| format!("failed to parse Wikipedia opensearch for `{query}`"))?;

        Ok(parse_wikipedia_opensearch_title(&value, query))
    }
}

pub fn is_google_challenge_page(input: &str) -> bool {
    let normalized = ascii_fold(input);
    GOOGLE_CHALLENGE_MARKERS
        .iter()
        .any(|marker| normalized.contains(&ascii_fold(marker)))
        || GOOGLE_CHALLENGE_ERROR_MARKERS
            .iter()
            .any(|marker| normalized.contains(&ascii_fold(marker)))
        || DUCKDUCKGO_CHALLENGE_MARKERS
            .iter()
            .any(|marker| normalized.contains(&ascii_fold(marker)))
}

fn is_duckduckgo_challenge_page(input: &str) -> bool {
    let normalized = ascii_fold(input);
    DUCKDUCKGO_CHALLENGE_MARKERS
        .iter()
        .any(|marker| normalized.contains(&ascii_fold(marker)))
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

fn build_duckduckgo_url(base_url: &str, query: &str) -> Result<Url> {
    let mut url = Url::parse(base_url)
        .with_context(|| format!("invalid fallback search base URL `{base_url}`"))?;
    url.query_pairs_mut().append_pair("q", query.trim());
    Ok(url)
}

fn build_wikipedia_opensearch_url(lang: &str, query: &str) -> Result<Url> {
    let mut url = Url::parse(&format!("https://{lang}.wikipedia.org/w/api.php"))
        .with_context(|| format!("invalid Wikipedia language `{lang}`"))?;
    url.query_pairs_mut()
        .append_pair("action", "opensearch")
        .append_pair("format", "json")
        .append_pair("namespace", "0")
        .append_pair("limit", "3")
        .append_pair("profile", "fuzzy")
        .append_pair("redirects", "resolve")
        .append_pair("search", query.trim());
    Ok(url)
}

fn build_wikipedia_summary_url(lang: &str, title: &str) -> Result<Url> {
    let encoded_title = urlencoding::encode(title.trim()).replace("%20", "_");
    Url::parse(&format!(
        "https://{lang}.wikipedia.org/api/rest_v1/page/summary/{encoded_title}"
    ))
    .with_context(|| format!("invalid Wikipedia summary URL for `{title}`"))
}

fn parse_google_search_html(html: &str, max_results: usize) -> SearchEnrichment {
    let document = Html::parse_document(html);
    let lines = visible_text_lines(&document);
    let snippets = extract_search_results(&document, max_results);
    let related_searches = filter_related_searches(extract_related_searches(&lines), &snippets);
    SearchEnrichment {
        ai_overview: extract_ai_overview(&lines),
        snippets,
        related_questions: extract_related_questions(&lines),
        related_searches,
    }
}

fn extract_ai_overview(lines: &[String]) -> Option<String> {
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

fn parse_duckduckgo_search_html(html: &str, max_results: usize) -> SearchEnrichment {
    let document = Html::parse_document(html);
    SearchEnrichment {
        ai_overview: None,
        snippets: extract_duckduckgo_results(&document, max_results),
        related_questions: Vec::new(),
        related_searches: Vec::new(),
    }
}

fn parse_wikipedia_opensearch_title(value: &Value, query: &str) -> Option<String> {
    let titles = value.get(1)?.as_array()?;

    titles
        .iter()
        .filter_map(Value::as_str)
        .find(|title| {
            let folded = ascii_fold(title);
            !folded.contains("desambiguacao")
                && !folded.contains("disambiguation")
                && title_matches_query(query, title)
        })
        .map(str::to_string)
}

fn parse_wikipedia_summary(value: &Value, lang: &str) -> Option<SearchSnippet> {
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .map(normalize_text)
        .filter(|value| !value.is_empty())?;
    let extract = value
        .get("extract")
        .and_then(Value::as_str)
        .map(normalize_text)
        .filter(|value| !value.is_empty())?;
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .map(normalize_text)
        .filter(|value| !value.is_empty());
    let page_url = value
        .pointer("/content_urls/desktop/page")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "https://{lang}.wikipedia.org/wiki/{}",
                urlencoding::encode(&title).replace("%20", "_")
            )
        });
    let snippet = match description {
        Some(description) if !ascii_fold(&extract).contains(&ascii_fold(&description)) => {
            format!("{description}. {extract}")
        }
        _ => extract,
    };

    Some(SearchSnippet {
        title: format!("{title} - Wikipedia"),
        url: page_url,
        domain: format!("{lang}.wikipedia.org"),
        snippet,
    })
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

fn extract_duckduckgo_results(document: &Html, max_results: usize) -> Vec<SearchSnippet> {
    let containers = selector_list(&[
        "div.result",
        "div.web-result",
        "div.result__body",
        "article",
    ]);
    let title_selectors = selector_list(&[
        "a.result__a",
        ".result__title a",
        "h2 a",
        "a[data-testid='result-title-a']",
    ]);
    let snippet_selectors = selector_list(&[
        "a.result__snippet",
        ".result__snippet",
        ".result__body .result__snippet",
        "[data-result='snippet']",
    ]);
    let link_selector = Selector::parse("a[href]").expect("valid anchor selector");
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for container_selector in &containers {
        for container in document.select(container_selector) {
            let mut title = title_selectors
                .iter()
                .find_map(|selector| first_text_in(&container, selector))
                .unwrap_or_default();
            let link = title_selectors
                .iter()
                .find_map(|selector| {
                    container.select(selector).find_map(|element| {
                        normalize_duckduckgo_href(element.value().attr("href")?)
                    })
                })
                .or_else(|| {
                    container.select(&link_selector).find_map(|element| {
                        normalize_duckduckgo_href(element.value().attr("href")?)
                    })
                });
            let Some(link) = link else {
                continue;
            };

            let snippet = snippet_selectors
                .iter()
                .find_map(|selector| first_text_in(&container, selector))
                .unwrap_or_default();

            if title.is_empty() {
                title = domain_from_url(&link);
            }
            if title.is_empty() && snippet.is_empty() {
                continue;
            }

            let key = format!("{title}::{link}");
            if !seen.insert(key) {
                continue;
            }

            items.push(SearchSnippet {
                title,
                url: link.clone(),
                domain: domain_from_url(&link),
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
        related_questions: merge_text_entries(
            primary.related_questions,
            fallback.related_questions,
            3,
        ),
        related_searches: merge_text_entries(
            primary.related_searches,
            fallback.related_searches,
            4,
        ),
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

fn supporting_point_from_snippet(item: &SearchSnippet) -> Option<String> {
    let summary = if item.snippet.is_empty() {
        item.title.clone()
    } else if item.title.is_empty() || ascii_fold(&item.snippet).contains(&ascii_fold(&item.title))
    {
        item.snippet.clone()
    } else {
        format!("{}: {}", item.title, item.snippet)
    };

    let summary = truncate_chars(summary.trim(), 180);
    if summary.is_empty() {
        None
    } else {
        Some(summary)
    }
}

fn ensure_sentence(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?') {
        trimmed.to_string()
    } else {
        format!("{trimmed}.")
    }
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let total = input.chars().count();
    if total <= max_chars {
        return input.trim().to_string();
    }

    let mut truncated = input
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    while truncated.ends_with(' ') || truncated.ends_with('.') || truncated.ends_with(',') {
        truncated.pop();
    }
    format!("{}…", truncated.trim_end())
}

fn filter_related_searches(
    related_searches: Vec<String>,
    snippets: &[SearchSnippet],
) -> Vec<String> {
    let snippet_titles = snippets
        .iter()
        .map(|item| ascii_fold(&item.title))
        .filter(|title| !title.is_empty())
        .collect::<HashSet<_>>();

    related_searches
        .into_iter()
        .filter(|item| !snippet_titles.contains(&ascii_fold(item)))
        .collect()
}

fn extract_related_questions(lines: &[String]) -> Vec<String> {
    collect_lines_after_label(lines, &RELATED_QUESTIONS_LABELS, 3, |line| {
        let trimmed = line.trim();
        trimmed.chars().count() >= 8
            && trimmed.chars().count() <= 140
            && (trimmed.ends_with('?')
                || trimmed.starts_with("como ")
                || trimmed.starts_with("o que ")
                || trimmed.starts_with("what ")
                || trimmed.starts_with("how "))
    })
}

fn extract_related_searches(lines: &[String]) -> Vec<String> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for (index, line) in lines.iter().enumerate() {
        if !matches_any_label(line, &RELATED_SEARCHES_LABELS) {
            continue;
        }

        for offset in index + 1..lines.len() {
            let candidate = lines[offset].trim();
            if candidate.is_empty() {
                continue;
            }
            if is_known_search_section_label(candidate) {
                break;
            }

            let next = lines
                .get(offset + 1)
                .map(|value| value.as_str())
                .unwrap_or("");
            if looks_like_search_result_title(candidate, next) && !items.is_empty() {
                break;
            }

            if !is_plausible_related_search(candidate) {
                if !items.is_empty() {
                    break;
                }
                continue;
            }

            let folded = ascii_fold(candidate);
            if !seen.insert(folded) {
                continue;
            }

            items.push(candidate.to_string());
            if items.len() >= 4 {
                return items;
            }
        }
    }

    items
}

fn collect_lines_after_label<F>(
    lines: &[String],
    labels: &[&str],
    max_items: usize,
    predicate: F,
) -> Vec<String>
where
    F: Fn(&str) -> bool,
{
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for (index, line) in lines.iter().enumerate() {
        if !matches_any_label(line, labels) {
            continue;
        }

        for candidate in lines.iter().skip(index + 1) {
            if candidate.is_empty() {
                continue;
            }
            if is_known_search_section_label(candidate) {
                break;
            }
            if !predicate(candidate) {
                continue;
            }

            let folded = ascii_fold(candidate);
            if !seen.insert(folded) {
                continue;
            }

            items.push(candidate.clone());
            if items.len() >= max_items {
                return items;
            }
        }
    }

    items
}

fn is_plausible_related_search(line: &str) -> bool {
    let trimmed = line.trim();
    let word_count = trimmed.split_whitespace().count();
    trimmed.chars().count() >= 3
        && trimmed.chars().count() <= 96
        && (2..=10).contains(&word_count)
        && !trimmed.ends_with('?')
}

fn looks_like_search_result_title(line: &str, next_line: &str) -> bool {
    let trimmed = line.trim();
    let next = next_line.trim();

    trimmed.split_whitespace().count() <= 6
        && trimmed.chars().any(|ch| ch.is_uppercase())
        && next.chars().count() >= 36
        && (next.ends_with('.') || next.contains(':'))
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

fn normalize_duckduckgo_href(href: &str) -> Option<String> {
    let absolute = if href.starts_with("//") {
        format!("https:{href}")
    } else if href.starts_with('/') {
        format!("https://duckduckgo.com{href}")
    } else {
        href.to_string()
    };

    let url = Url::parse(&absolute).ok()?;
    let host = url.host_str().unwrap_or_default();

    if host.ends_with("duckduckgo.com") && url.path().starts_with("/l/") {
        let target = url.query_pairs().find_map(|(name, value)| {
            if name == "uddg" {
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

    if absolute.starts_with("http://") || absolute.starts_with("https://") {
        if host.ends_with("duckduckgo.com") {
            return None;
        }
        return Some(absolute);
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

fn merge_text_entries(
    primary: Vec<String>,
    fallback: Vec<String>,
    max_items: usize,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for item in primary.into_iter().chain(fallback) {
        let folded = ascii_fold(&item);
        if !seen.insert(folded) {
            continue;
        }
        items.push(item);
        if items.len() >= max_items {
            break;
        }
    }

    items
}

fn wikipedia_search_candidates(query: &str) -> Vec<String> {
    let trimmed = query
        .trim()
        .trim_matches(|ch: char| matches!(ch, '?' | '!' | '.' | ':' | ';' | '"' | '\''));
    let mut candidates = Vec::new();
    for alias in knowledge_alias_candidates(trimmed) {
        push_candidate(&mut candidates, alias);
    }

    let cleaned = cleaned_knowledge_query(trimmed);
    for alias in knowledge_alias_candidates(&cleaned) {
        push_candidate(&mut candidates, alias);
    }

    push_candidate(&mut candidates, cleaned);
    push_candidate(&mut candidates, trimmed.to_string());

    candidates
}

fn knowledge_alias_candidates(query: &str) -> Vec<String> {
    let folded = ascii_fold(query);
    let mut aliases = Vec::new();

    if (folded.contains("nitch") || folded.contains("nietz") || folded.contains("nietch"))
        && (folded.contains("freder") || folded.contains("friedr"))
    {
        aliases.push("Friedrich Nietzsche".to_string());
    }

    if folded.contains("rousseau") || folded.contains("rosseau") {
        aliases.push("Jean-Jacques Rousseau".to_string());
    }

    aliases
}

fn cleaned_knowledge_query(query: &str) -> String {
    let normalized = ascii_fold(query);
    let prefixes = [
        "pesquise sobre ",
        "pesquisar sobre ",
        "pesquise ",
        "pesquisar ",
        "busque sobre ",
        "procure sobre ",
        "quem foi ",
        "quem e ",
        "quem é ",
        "o que e ",
        "o que é ",
        "o que significa ",
        "explique ",
        "resuma ",
        "quando foi fundada a ",
        "quando foi fundado o ",
        "quando foi fundada ",
        "quando foi fundado ",
        "qual e a ",
        "qual é a ",
        "qual a ",
        "qual e o ",
        "qual é o ",
        "qual o ",
    ];

    for prefix in prefixes {
        if normalized.starts_with(&ascii_fold(prefix)) {
            let prefix_len = prefix.chars().count();
            return query
                .chars()
                .skip(prefix_len)
                .collect::<String>()
                .trim()
                .to_string();
        }
    }

    query.trim().to_string()
}

fn push_candidate(candidates: &mut Vec<String>, candidate: String) {
    let candidate = normalize_text(&candidate);
    if candidate.is_empty() {
        return;
    }
    let folded = ascii_fold(&candidate);
    if candidates.iter().any(|item| ascii_fold(item) == folded) {
        return;
    }
    candidates.push(candidate);
}

fn title_matches_query(query: &str, title: &str) -> bool {
    let query = ascii_fold(query);
    let title = ascii_fold(title);
    let query = query.replace("?", " ").replace("!", " ").replace(".", " ");
    let title = title.replace("(", " ").replace(")", " ").replace("-", " ");
    let query_tokens = meaningful_tokens(&query);
    let title_tokens = meaningful_tokens(&title);

    if query_tokens.is_empty() || title_tokens.is_empty() {
        return false;
    }

    let query_joined = query_tokens.join(" ");
    let title_joined = title_tokens.join(" ");
    if title_joined.contains(&query_joined) || query_joined.contains(&title_joined) {
        return true;
    }

    let overlap = query_tokens
        .iter()
        .filter(|token| title_tokens.contains(token))
        .count();
    let min_len = query_tokens.len().min(title_tokens.len());
    overlap >= min_len.max(1)
}

fn meaningful_tokens(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .map(|token| {
            token
                .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
                .to_string()
        })
        .filter(|token| {
            token.chars().count() >= 3
                && !matches!(
                    token.as_str(),
                    "quem"
                        | "que"
                        | "foi"
                        | "fundada"
                        | "fundado"
                        | "quando"
                        | "qual"
                        | "com"
                        | "por"
                        | "para"
                        | "uma"
                        | "uns"
                        | "das"
                        | "dos"
                        | "the"
                        | "who"
                        | "what"
                        | "when"
                        | "was"
                )
        })
        .collect()
}

fn is_ai_overview_label(line: &str) -> bool {
    matches_any_label(line, &AI_OVERVIEW_LABELS)
}

fn is_ai_overview_stop_marker(line: &str) -> bool {
    let normalized = ascii_fold(line);
    AI_OVERVIEW_STOP_MARKERS
        .iter()
        .any(|marker| normalized.contains(&ascii_fold(marker)))
}

fn is_known_search_section_label(line: &str) -> bool {
    is_ai_overview_label(line)
        || matches_any_label(line, &RELATED_QUESTIONS_LABELS)
        || matches_any_label(line, &RELATED_SEARCHES_LABELS)
        || is_ai_overview_stop_marker(line)
}

fn matches_any_label(line: &str, labels: &[&str]) -> bool {
    let normalized = ascii_fold(line);
    labels
        .iter()
        .any(|label| normalized.contains(&ascii_fold(label)))
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
              <div>Visão geral criada por IA</div>
              <div>O daemon coordena clipboard, TTS e inferencia local.</div>
              <div>Ele pode abrir a pesquisa para aprofundamento.</div>
              <div>As pessoas tambem perguntam</div>
              <div>Como configurar o VisionClip?</div>
              <div>Como ativar o Piper?</div>
              <div>Pesquisas relacionadas</div>
              <div>visionclip piper setup</div>
              <div>visionclip daemon config</div>
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
        assert_eq!(enrichment.related_questions.len(), 2);
        assert_eq!(enrichment.related_searches.len(), 2);
    }

    #[test]
    fn parse_duckduckgo_html_extracts_fallback_results() {
        let html = r#"
            <html><body>
              <div class="result">
                <h2 class="result__title">
                  <a class="result__a" href="/l/?uddg=https%3A%2F%2Fdeveloper.mozilla.org%2Fpt-BR%2Fdocs%2FWeb%2FJavaScript">JavaScript - MDN</a>
                </h2>
                <a class="result__snippet">JavaScript é uma linguagem de programação usada na Web.</a>
              </div>
              <div class="result">
                <h2 class="result__title">
                  <a class="result__a" href="https://pt.wikipedia.org/wiki/JavaScript">JavaScript - Wikipédia</a>
                </h2>
                <a class="result__snippet">JavaScript permite páginas interativas.</a>
              </div>
            </body></html>
        "#;

        let enrichment = parse_duckduckgo_search_html(html, 3);
        assert_eq!(enrichment.snippets.len(), 2);
        assert_eq!(enrichment.snippets[0].title, "JavaScript - MDN");
        assert_eq!(
            enrichment.snippets[0].url,
            "https://developer.mozilla.org/pt-BR/docs/Web/JavaScript"
        );
        assert_eq!(enrichment.snippets[0].domain, "developer.mozilla.org");
        assert!(enrichment.spoken_text("O que é Javascript?").is_some());
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
            related_questions: vec!["Como instalar?".into()],
            related_searches: vec!["visionclip install".into()],
        };

        let text = enrichment
            .clipboard_text("erro visionclip portal")
            .expect("clipboard summary");
        assert!(text.contains("Leitura inicial"));
        assert!(text.contains("Pistas iniciais"));
        assert!(text.contains("Fonte 1"));
        assert!(text.contains("Perguntas para aprofundar"));
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
            related_questions: vec!["Como configurar?".into()],
            related_searches: vec!["visionclip config".into()],
        };
        let fallback = SearchEnrichment {
            ai_overview: None,
            snippets: vec![SearchSnippet {
                title: "Forum".into(),
                url: "https://example.org/forum".into(),
                domain: "example.org".into(),
                snippet: "Discussao complementar.".into(),
            }],
            related_questions: vec!["Como configurar?".into(), "Como testar?".into()],
            related_searches: vec!["visionclip config".into(), "visionclip test".into()],
        };

        let merged = merge_enrichment(primary, fallback, 3);
        assert_eq!(merged.ai_overview, Some("Resumo principal".into()));
        assert_eq!(merged.snippets.len(), 2);
        assert_eq!(merged.snippets[1].title, "Forum");
        assert_eq!(merged.related_questions.len(), 2);
        assert_eq!(merged.related_searches.len(), 2);
    }

    #[test]
    fn spoken_text_falls_back_to_structured_snippet_summary() {
        let enrichment = SearchEnrichment {
            ai_overview: None,
            snippets: vec![SearchSnippet {
                title: "Wikipedia".into(),
                url: "https://example.com/wiki".into(),
                domain: "example.com".into(),
                snippet: "Artigo introdutório sobre o tema pesquisado.".into(),
            }],
            related_questions: Vec::new(),
            related_searches: Vec::new(),
        };

        let spoken = enrichment.spoken_text("tema geral").expect("spoken text");
        assert!(spoken.contains("Artigo introdutório"));
    }

    #[test]
    fn parse_google_html_discards_related_searches_that_match_result_titles() {
        let html = r#"
            <html><body>
              <div>Pesquisas relacionadas</div>
              <div>VisionClip Docs</div>
              <div>visionclip piper setup</div>
              <div class="g">
                <a href="/url?q=https://example.com/docs&sa=U"><h3>VisionClip Docs</h3></a>
                <div class="VwiC3b">Guia de configuracao do daemon e do Piper.</div>
              </div>
            </body></html>
        "#;

        let enrichment = parse_google_search_html(html, 3);
        assert_eq!(
            enrichment.related_searches,
            vec!["visionclip piper setup".to_string()]
        );
    }

    #[test]
    fn spoken_text_uses_follow_up_question_when_available() {
        let enrichment = SearchEnrichment {
            ai_overview: Some("Resumo curto sobre o tema.".into()),
            snippets: Vec::new(),
            related_questions: vec!["Como isso funciona na prática?".into()],
            related_searches: Vec::new(),
        };

        let spoken = enrichment.spoken_text("tema geral").expect("spoken text");
        assert!(spoken.contains("Resumo curto sobre o tema."));
        assert!(spoken.contains("Como isso funciona na prática?"));
    }

    #[test]
    fn detects_google_challenge_page() {
        let html = r#"
            <html><body>
              <noscript><meta content="0;url=/httpservice/retry/enablejs?sei=123" http-equiv="refresh"></noscript>
              <div id="yvlrue">If you're having trouble accessing Google Search, please click here.</div>
            </body></html>
        "#;

        assert!(is_google_challenge_page(html));
    }

    #[test]
    fn detects_duckduckgo_challenge_page() {
        let html = r#"
            <html><body>
              <form id="challenge-form" action="//duckduckgo.com/anomaly.js">
                <div class="anomaly-modal__title">Unfortunately, bots use DuckDuckGo too.</div>
              </form>
            </body></html>
        "#;

        assert!(is_duckduckgo_challenge_page(html));
        assert!(is_google_challenge_page(html));
    }

    #[test]
    fn knowledge_query_cleanup_extracts_subject() {
        assert_eq!(
            wikipedia_search_candidates("Quem foi Rousseau?")[0],
            "Jean-Jacques Rousseau"
        );
        assert_eq!(
            wikipedia_search_candidates("Quando foi fundada a NASA?")[0],
            "NASA"
        );
        assert_eq!(
            wikipedia_search_candidates("O que é JavaScript?")[0],
            "JavaScript"
        );
    }

    #[test]
    fn parse_wikipedia_opensearch_prefers_article_title() {
        let value: Value = serde_json::from_str(
            r#"["Rousseau",["Rousseau (desambiguação)","Jean-Jacques Rousseau"],["",""],["",""]]"#,
        )
        .unwrap();

        assert_eq!(
            parse_wikipedia_opensearch_title(&value, "Rousseau"),
            Some("Jean-Jacques Rousseau".to_string())
        );
    }

    #[test]
    fn parse_wikipedia_opensearch_rejects_low_similarity_title() {
        let value: Value = serde_json::from_str(
            r#"["Frederich Nitch",["Freddie Pitcher"],[""],["https://pt.wikipedia.org/wiki/Freddie_Pitcher"]]"#,
        )
        .unwrap();

        assert_eq!(
            parse_wikipedia_opensearch_title(&value, "Frederich Nitch"),
            None
        );
    }

    #[test]
    fn knowledge_aliases_cover_common_voice_misspellings() {
        assert_eq!(
            wikipedia_search_candidates("Quem é Frederich Nitch?")[0],
            "Friedrich Nietzsche"
        );
        assert_eq!(
            wikipedia_search_candidates("Quem foi Rosseau?")[0],
            "Jean-Jacques Rousseau"
        );
    }

    #[test]
    fn parse_wikipedia_summary_extracts_snippet() {
        let value: Value = serde_json::from_str(
            r#"{
                "title": "Jean-Jacques Rousseau",
                "description": "filósofo genebrino",
                "extract": "Jean-Jacques Rousseau foi um importante filósofo, teórico político e escritor.",
                "content_urls": {
                    "desktop": {
                        "page": "https://pt.wikipedia.org/wiki/Jean-Jacques_Rousseau"
                    }
                }
            }"#,
        )
        .unwrap();

        let snippet = parse_wikipedia_summary(&value, "pt").expect("summary");
        assert_eq!(snippet.title, "Jean-Jacques Rousseau - Wikipedia");
        assert_eq!(snippet.domain, "pt.wikipedia.org");
        assert!(snippet.snippet.contains("filósofo genebrino"));
        assert!(snippet.url.ends_with("Jean-Jacques_Rousseau"));
    }
}
