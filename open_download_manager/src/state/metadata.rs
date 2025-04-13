use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SegmentMetadata {
    pub segment_id: usize,
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub part_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadMetadata {
    pub url: String,
    pub output_path: String,
    pub total_size: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub segments: Vec<SegmentMetadata>,
}

impl DownloadMetadata {
    pub fn save_to_file(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)
    }

    pub fn load_from_file(path: &str) -> std::io::Result<DownloadMetadata> {
        let contents = fs::read_to_string(path)?;
        let metadata: DownloadMetadata = serde_json::from_str(&contents)?;
        Ok(metadata)
    }

    pub fn exists(path: &str) -> bool {
        Path::new(path).exists()
    }
}