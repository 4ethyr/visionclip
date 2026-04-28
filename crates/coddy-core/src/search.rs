use serde::{Deserialize, Serialize};

use crate::ExtractionSource;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SearchProvider {
    Google,
    DuckDuckGo,
    Brave,
    Custom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SearchExtractionPolicy {
    RenderedVisibleOnly,
    ProviderApiOnly,
    HybridVisibleThenOrganic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SourceQuality {
    Primary,
    Official,
    ReputableSecondary,
    UserGenerated,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResultItem {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub rank: usize,
    pub source_quality: SourceQuality,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResultContext {
    pub query: String,
    pub provider: SearchProvider,
    pub organic_results: Vec<SearchResultItem>,
    pub ai_overview_text: Option<String>,
    pub ai_overview_sources: Vec<SearchResultItem>,
    pub visible_text: String,
    pub captured_at_unix_ms: i64,
    pub confidence: f32,
    pub source_urls: Vec<String>,
    pub extraction_method: ExtractionSource,
}

impl SearchResultContext {
    pub fn has_ai_overview(&self) -> bool {
        self.ai_overview_text
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty())
    }

    pub fn source_count(&self) -> usize {
        self.source_urls.len().max(self.organic_results.len())
    }
}
