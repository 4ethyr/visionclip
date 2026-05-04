#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SearchMetrics {
    pub last_query_ms: u32,
    pub last_indexed_files: usize,
    pub last_errors: usize,
}
