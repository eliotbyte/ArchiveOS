use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BlobGcReport {
    pub scanned: u32,
    pub candidates: u32,
    pub removed: u32,
    pub bytes_freed: u64,
    pub errors: Vec<String>,
}
