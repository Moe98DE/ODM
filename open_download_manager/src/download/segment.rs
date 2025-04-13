use reqwest::blocking::Client;
use reqwest::header::{RANGE, USER_AGENT, IF_RANGE};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};
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
}

impl<'a> DownloadSegment<'a> {
    pub fn new(
        url: String,
        meta: SegmentMetadata,
        tracker: Arc<Mutex<SegmentedProgressTracker>>,
        config: &'a Config,
        etag: Option<String>,
    ) -> Self {
        Self {
            url,
            meta,
            tracker,
            config,
            etag,
        }
    }    

    pub fn download(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut downloaded = get_downloaded_size(&self.meta.part_path)?;
        if downloaded >= (self.meta.end - self.meta.start + 1) {
            println!("✔️ Segment {} already completed.", self.meta.segment_id);
            return Ok(());
        }

        let start = self.meta.start + downloaded;
        let range_header = format!("bytes={}-{}", start, self.meta.end);

        for attempt in 1..=self.config.max_retries {
            println!(
                "📡 Segment {} resuming (attempt {}/{}) from byte {}",
                self.meta.segment_id, attempt, self.config.max_retries, start
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

            let response_result = request.send();

            let mut response = match response_result {
                Ok(res) if res.status().is_success() || res.status().as_u16() == 206 => res,
                Ok(res) => {
                    eprintln!(
                        "❌ Segment {} failed with status {}",
                        self.meta.segment_id,
                        res.status()
                    );
                    continue;
                }
                Err(err) => {
                    eprintln!("❌ Segment {} request error: {}", self.meta.segment_id, err);
                    continue;
                }
            };

            let write_result = (|| -> Result<(), Box<dyn std::error::Error>> {
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.meta.part_path)?;

                let mut buffer = [0; 8192];
                let mut total_written = downloaded;

                loop {
                    let n = response.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    file.write_all(&buffer[..n])?;
                    total_written += n as u64;

                    let mut tracker = self.tracker.lock().unwrap();
                    tracker.update(self.meta.segment_id, n as u64);
                }

                Ok(())
            })();

            match write_result {
                Ok(_) => return Ok(()),
                Err(err) => {
                    eprintln!("❌ Segment {} write error: {}", self.meta.segment_id, err);
                }
            }
        }

        Err(format!("Segment {} failed after retries", self.meta.segment_id).into())
    }
}

fn get_downloaded_size(path: &str) -> io::Result<u64> {
    if let Ok(metadata) = std::fs::metadata(path) {
        Ok(metadata.len())
    } else {
        Ok(0)
    }
}