use crate::config::Config;
use crate::download::progress::SegmentedProgressTracker;
use crate::download::segment::DownloadSegment;
use crate::download::single;
use crate::state::metadata::{DownloadMetadata, SegmentMetadata};
use crate::core::manager::DownloadStatus;

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use reqwest::blocking::Client;

pub fn download_file_segmented(
    url: &str,
    output_path: &str,
    num_threads: usize,
    config: &Config,
    external_pause_flag: Option<Arc<AtomicBool>>,
    external_status: Option<Arc<Mutex<DownloadStatus>>>,
    external_tracker: Option<Arc<Mutex<SegmentedProgressTracker>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let url_hash = hash_url(url);
    let meta_path = format!("downloads/meta/{}.meta.json", url_hash);
    fs::create_dir_all("downloads/meta")?;

    let mut metadata: DownloadMetadata;

    if DownloadMetadata::exists(&meta_path) {
        println!("🔄 Resuming download from metadata...");
        metadata = DownloadMetadata::load_from_file(&meta_path)?;
    } else {
        let client = Client::new();
        let res = client.head(url).send()?.error_for_status()?;

        if res.headers().get("accept-ranges").is_none() {
            println!("⚠️ Server does not support segmented downloading.");
            return single::download_file(url, output_path);
        }

        let total_size = match res.headers().get("content-length") {
            Some(len) => len.to_str()?.parse::<u64>()?,
            None => {
                println!("⚠️ No Content-Length. Falling back.");
                return single::download_file(url, output_path);
            }
        };

        let etag = res.headers().get("etag").map(|v| v.to_str().unwrap_or("").to_string());
        let last_modified = res.headers().get("last-modified").map(|v| v.to_str().unwrap_or("").to_string());

        let chunk_size = total_size / num_threads as u64;
        let mut segments = Vec::new();

        for i in 0..num_threads {
            let start = i as u64 * chunk_size;
            let end = if i == num_threads - 1 {
                total_size - 1
            } else {
                (i as u64 + 1) * chunk_size - 1
            };

            segments.push(SegmentMetadata {
                segment_id: i,
                start,
                end,
                downloaded: 0,
                part_path: format!("{}.part{}", output_path, i),
            });
        }

        metadata = DownloadMetadata {
            url: url.to_string(),
            output_path: output_path.to_string(),
            total_size,
            etag,
            last_modified,
            segments,
        };

        metadata.save_to_file(&meta_path)?;
    }

    println!("📦 Total size: {} bytes", metadata.total_size);
    println!("🧵 Threads: {}", num_threads);

    let pause_flag = external_pause_flag.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    let status = external_status.unwrap_or_else(|| Arc::new(Mutex::new(DownloadStatus::Idle)));
    let tracker = external_tracker.unwrap_or_else(|| {
        Arc::new(Mutex::new(SegmentedProgressTracker::new(
            metadata.segments.len(),
            metadata.total_size / metadata.segments.len() as u64,
            metadata.total_size,
        )))
    });

    let pause_flag_for_signal = pause_flag.clone();
    ctrlc::set_handler(move || {
        println!("\n🛑 Ctrl+C received. Pausing download...");
        pause_flag_for_signal.store(true, Ordering::Relaxed);
    }).expect("Failed to set Ctrl+C handler");

    let metadata_for_saving = metadata.clone();
    let meta_path_clone = meta_path.clone();
    let pause_flag_for_saving = pause_flag.clone();
    thread::spawn(move || {
        while !pause_flag_for_saving.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_secs(5));
            if let Err(e) = metadata_for_saving.save_to_file(&meta_path_clone) {
                eprintln!("⚠️ Auto-save failed: {}", e);
            } else {
                println!("💾 Auto-saved metadata");
            }
        }
    });

    let mut handles = vec![];

    for segment_meta in metadata.segments.clone() {
        let tracker_clone = Arc::clone(&tracker);
        let config_clone = config.clone();
        let url_clone = metadata.url.clone();
        let etag = metadata.etag.clone();
        let pause_flag_clone = pause_flag.clone();

        let handle = thread::spawn(move || {
            let segment = DownloadSegment::new(
                url_clone,
                segment_meta,
                tracker_clone,
                &config_clone,
                etag,
                pause_flag_clone,
            );

            if let Err(e) = segment.download() {
                eprintln!("❌ Segment {} failed: {}", segment.meta.segment_id, e);
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    if pause_flag.load(Ordering::Relaxed) {
        println!("💾 Saving metadata on pause...");
        metadata.save_to_file(&meta_path)?;
        println!("⏸️ Download paused.");
        return Ok(());
    }

    merge_files(output_path, metadata.segments.len())?;
    fs::remove_file(&meta_path).ok();
    println!("\n✅ All segments merged to: {}", output_path);

    Ok(())
}

fn merge_files(output_path: &str, num_parts: usize) -> io::Result<()> {
    let mut output = File::create(output_path)?;

    for i in 0..num_parts {
        let part_path = format!("{}.part{}", output_path, i);
        let mut part_file = File::open(&part_path)?;
        io::copy(&mut part_file, &mut output)?;
        fs::remove_file(&part_path)?;
    }

    Ok(())
}

pub fn hash_url(url: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    hex::encode(hasher.finalize())
}