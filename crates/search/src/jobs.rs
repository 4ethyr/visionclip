#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SearchJobType {
    Metadata,
    ExtractText,
    Ocr,
    Embed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SearchJobStatus {
    Queued,
    Running,
    Failed,
    Done,
}
