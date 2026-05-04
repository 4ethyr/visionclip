#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VectorBackend {
    Disabled,
    SqliteVec,
    LanceDb,
    Qdrant,
}
