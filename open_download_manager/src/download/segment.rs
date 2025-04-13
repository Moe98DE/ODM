use reqwest::blocking::Client;
use reqwest::header::{RANGE, USER_AGENT, IF_RANGE};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::config::Config;
use crate::download::progress::SegmentedProgressTracker;
use crate::state::metadata::SegmentMetadata;

pub struct DownloadSegment<'a> {
    pub url: String,
    pub meta: SegmentMetadata,
    pub tracker: Arc<Mutex<SegmentedProgressTracker>>,
    pub config: &'a Config,
    pub etag: Option<String>,
    pub pause_flag: Arc<AtomicBool>,
}

impl<'a> DownloadSegment<'a> {
    pub fn new(
        url: String,
        meta: SegmentMetadata,
        tracker: Arc<Mutex<SegmentedProgressTracker>>,
        config: &'a Config,
        etag: Option<String>,
        pause_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            url,
            meta,
            tracker,
            config,
            etag,
            pause_flag,
        }
    }

    pub fn download(&self) -> Result<(), Box<dyn std::error::Error>> {
        let downloaded = get_downloaded_size(&self.meta.part_path)?;
        if downloaded >= (self.meta.end - self.meta.start + 1) {
            println!("✔️ Segment {} already done.", self.meta.segment_id);
            return Ok(());
        }

        let start = self.meta.start + downloaded;
        let range_header = format!("bytes={}-{}", start, self.meta.end);

        for attempt in 1..=self.config.max_retries {
            println!(
                "📡 Segment {} downloading (attempt {}/{})...",
                self.meta.segment_id, attempt, self.config.max_retries
            );

            let client = Client::builder()
                .timeout(Duration::from_secs(self.config.timeout_secs))
                .build()?;

            let mut request = client
                .get(&self.url)
                .header(RANGE, &range_header)
                .header(USER_AGENT, "OpenDownloadManager/0.1");

            if let Some(etag) = &self.etag {
                request = request.header(IF_RANGE, etag);
            }

            let mut response = match request.send() {
                Ok(res) if res.status().is_success() || res.status().as_u16() == 206 => res,
                Ok(res) => {
                    eprintln!(
                        "❌ Segment {} HTTP error: {}",
                        self.meta.segment_id,
                        res.status()
                    );
                    continue;
                }
                Err(e) => {
                    eprintln!("❌ Segment {} network error: {}", self.meta.segment_id, e);
                    continue;
                }
            };

            let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.meta.part_path)?;

                let mut buffer = [0; 8192];

                loop {
                    if self.pause_flag.load(Ordering::Relaxed) {
                        println!("⏸️ Segment {} paused", self.meta.segment_id);
                        return Ok(());
                    }

                    let n = response.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }

                    file.write_all(&buffer[..n])?;

                    let mut tracker = self.tracker.lock().unwrap();
                    tracker.update(self.meta.segment_id, n as u64);
                }

                Ok(())
            })();

            if result.is_ok() {
                return Ok(());
            }
        }

        Err(format!(
            "Segment {} failed after {} attempts",
            self.meta.segment_id, self.config.max_retries
        )
        .into())
    }
}

fn get_downloaded_size(path: &str) -> io::Result<u64> {
    std::fs::metadata(path).map(|m| m.len()).or(Ok(0))
}
