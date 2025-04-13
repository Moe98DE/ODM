use std::collections::HashMap;
use std::io::{self, Write};

/// Used by the single-threaded downloader
pub struct SimpleProgressTracker {
    total_size: u64,
    downloaded: u64,
}

impl SimpleProgressTracker {
    pub fn new(total_size: u64) -> Self {
        Self {
            total_size,
            downloaded: 0,
        }
    }

    pub fn update(&mut self, bytes: u64) {
        self.downloaded += bytes;
        let percent = (self.downloaded as f64 / self.total_size as f64) * 100.0;
        print!("\r⏬ Downloading: {:.2}%", percent);
        io::stdout().flush().unwrap();
    }
}

/// Used by the segmented (multi-threaded) downloader
pub struct SegmentedProgressTracker {
    pub segments: HashMap<usize, (u64, u64)>, // segment_id: (downloaded, total)
    pub total_downloaded: u64,
    pub total_size: u64,
}

impl SegmentedProgressTracker {
    pub fn new(num_segments: usize, segment_size: u64, total_size: u64) -> Self {
        let mut segments = HashMap::new();
        for i in 0..num_segments {
            segments.insert(i, (0, segment_size));
        }
        Self {
            segments,
            total_downloaded: 0,
            total_size,
        }
    }

    pub fn update(&mut self, segment_id: usize, bytes: u64) {
        if let Some((downloaded, _)) = self.segments.get_mut(&segment_id) {
            *downloaded += bytes;
        }
        self.total_downloaded += bytes;
        self.display();
    }

    pub fn display(&self) {
        let percent = (self.total_downloaded as f64 / self.total_size as f64) * 100.0;
        print!("\r⏬ Overall Progress: {:.2}%", percent);
        io::stdout().flush().unwrap();
    }
}
