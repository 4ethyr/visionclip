#[derive(Debug, Clone, Default)]
pub struct SqliteVecStatus {
    pub available: bool,
    pub embedding_count: usize,
}
