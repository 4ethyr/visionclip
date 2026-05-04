#[derive(Debug, Clone, Default)]
pub struct LanceDbStatus {
    pub available: bool,
    pub table_count: usize,
}
