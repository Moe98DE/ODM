use crate::config::Config;
use crate::download::manager::download_file_segmented;
use crate::download::progress::SegmentedProgressTracker;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool};
use std::thread::JoinHandle;

use std::sync::atomic::Ordering;

#[derive(Debug, PartialEq)]
pub enum DownloadStatus {
    Idle,
    Downloading,
    Paused,
    Completed,
    Canceled,
    Retrying,
    Failed(String),
}


impl std::fmt::Display for DownloadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadStatus::Idle => write!(f, "Idle"),
            DownloadStatus::Downloading => write!(f, "Downloading"),
            DownloadStatus::Paused => write!(f, "Paused"),
            DownloadStatus::Canceled => write!(f, "Canceled"),
            DownloadStatus::Retrying => write!(f, "Retrying"),
            DownloadStatus::Completed => write!(f, "Completed"),
            DownloadStatus::Failed(reason) => write!(f, "Failed ({})", reason),
        }
    }
}

#[derive(Debug)]
pub struct DownloadProgress {
    pub total_downloaded: u64,
    pub total_size: u64,
    pub percent: f64,
    pub per_segment: Vec<(usize, u64, u64)>, // segment_id, downloaded, total
}

pub struct DownloadTask {
    pub id: String,
    pub url: String,
    pub output_path: String,
    pub meta_path: String,
    pub handles: Vec<JoinHandle<()>>,
    pub pause_flag: Arc<AtomicBool>,
    pub status: Arc<Mutex<DownloadStatus>>,
    pub progress: Arc<Mutex<SegmentedProgressTracker>>,
}

pub struct DownloadManager {
    pub tasks: Mutex<HashMap<String, DownloadTask>>,
    pub config: Arc<Config>,
}

impl DownloadManager {
    pub fn new(config: Config) -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            config: Arc::new(config),
        }
    }

    pub fn list_downloads(&self) -> Vec<(String, String, String)> {
        let tasks = self.tasks.lock().unwrap();
        tasks
            .iter()
            .map(|(id, task)| {
                let status = task.status.lock().unwrap();
                (id.clone(), task.url.clone(), format!("{:?}", *status))
            })
            .collect()
    }
    

    pub fn get_progress(&self, id: &str) -> Option<DownloadProgress> {
        let tasks = self.tasks.lock().unwrap();
        let task = tasks.get(id)?;
        let tracker = task.progress.lock().unwrap();

        Some(DownloadProgress {
            total_downloaded: tracker.total_downloaded,
            total_size: tracker.total_size,
            percent: (tracker.total_downloaded as f64 / tracker.total_size as f64) * 100.0,
            per_segment: tracker.segments.iter()
                .map(|(id, (downloaded, total))| (*id, *downloaded, *total))
                .collect(),
        })
    }

    pub fn add_download(&self, url: String, output_path: String) -> String {
        let id = crate::download::manager::hash_url(&url);
        let pause_flag = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(DownloadStatus::Idle));
        let progress = Arc::new(Mutex::new(SegmentedProgressTracker::new(
            self.config.num_threads,
            0,
            0,
        )));

        let config_clone = Arc::clone(&self.config);
        let pause_flag_clone = Arc::clone(&pause_flag);
        let status_clone_download = Arc::clone(&status);
        let status_clone_set = Arc::clone(&status);
        let progress_clone = Arc::clone(&progress);

        let url_clone = url.clone();
        let output_path_clone = output_path.clone();

        let handle = std::thread::spawn(move || {
            *status_clone_download.lock().unwrap() = DownloadStatus::Downloading;
            match download_file_segmented(
                &url_clone,
                &output_path_clone,
                config_clone.num_threads,
                &config_clone,
                Some(pause_flag_clone),
                Some(status_clone_download),
                Some(progress_clone),
            ) {
                Ok(_) => *status_clone_set.lock().unwrap() = DownloadStatus::Completed,
                Err(e) => *status_clone_set.lock().unwrap() = DownloadStatus::Failed(e.to_string()),
            }
        });

        let task = DownloadTask {
            id: id.clone(),
            url,
            output_path,
            meta_path: format!("downloads/meta/{}.meta.json", id),
            handles: vec![handle],
            pause_flag,
            status,
            progress,
        };

        self.tasks.lock().unwrap().insert(id.clone(), task);
        id
    }

    pub fn pause(&self, id: &str) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.get(id) {
            task.pause_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            *task.status.lock().unwrap() = DownloadStatus::Paused;
            return true;
        }
        false
    }

    pub fn resume(&self, id: &str) -> bool {
        let (url, path, should_resume) = {
            let tasks = self.tasks.lock().unwrap();
            if let Some(task) = tasks.get(id) {
                let should_resume = *task.status.lock().unwrap() == DownloadStatus::Paused;
                (task.url.clone(), task.output_path.clone(), should_resume)
            } else {
                return false;
            }
        };

        if should_resume {
            self.add_download(url, path);
            true
        } else {
            false
        }
    }

    pub fn cancel(&self, id: &str) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.get_mut(id) {
            println!("❌ Canceling download {}", id);
            task.pause_flag.store(true, Ordering::Relaxed);
    
            // Wait for threads to exit
            for handle in task.handles.drain(..) {
                let _ = handle.join();
            }
    
            // Clean up files
            let _ = std::fs::remove_file(&task.meta_path);
            for i in 0..self.config.num_threads {
                let part_path = format!("{}.part{}", task.output_path, i);
                let _ = std::fs::remove_file(&part_path);
            }
    
            *task.status.lock().unwrap() = DownloadStatus::Canceled;
            return true;
        }
        false
    }

    pub fn retry(&self, id: &str) -> bool {
        let tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.get(id) {
            let current_status = task.status.lock().unwrap();
            match *current_status {
                DownloadStatus::Failed(_) | DownloadStatus::Canceled => {
                    // Allow retry
                }
                _ => return false, // Cannot retry if not failed or canceled
            }
    
            let url = task.url.clone();
            let output_path = task.output_path.clone();
            drop(current_status); // Drop lock before reuse
            drop(tasks);          // Drop entire lock to avoid deadlock
            println!("🔁 Retrying download {}", id);
    
            // Reuse add_download
            self.add_download(url, output_path);
            return true;
        }
        false
    }
    
}