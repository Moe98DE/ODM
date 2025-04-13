use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use open_download_manager::core::manager::{DownloadManager, DownloadStatus};
use open_download_manager::config::Config;


fn test_url() -> &'static str {
    "https://ash-speed.hetzner.com/1GB.bin"
}

fn test_output_path() -> &'static str {
    "test_download.bin"
}

#[test]
fn test_full_download_flow() {
    let config = Config {
        timeout_secs: 10,
        max_retries: 2,
        num_threads: 4,
        default_output_path: "".into(),
    };

    let manager = DownloadManager::new(config);
    let id = manager.add_download(test_url().into(), test_output_path().into());

    // Wait for some progress
    sleep(Duration::from_secs(5));

    // Check it's active
    let list = manager.list_downloads();
    let status = list.iter().find(|(i, _, _)| i == &id).unwrap().2.clone();
    assert!(status.contains("Downloading"));

    // Pause it
    assert!(manager.pause(&id));
    sleep(Duration::from_secs(1));
    let list = manager.list_downloads();
    assert!(list.iter().any(|(i, _, s)| i == &id && s.contains("Paused")));

    // Resume it
    assert!(manager.resume(&id));
    sleep(Duration::from_secs(1));

    // Cancel it
    assert!(manager.cancel(&id));
    let list = manager.list_downloads();
    assert!(list.iter().any(|(i, _, s)| i == &id && s.contains("Canceled")));

    // Retry it
    assert!(manager.retry(&id));
    sleep(Duration::from_secs(1));
    let list = manager.list_downloads();
    assert!(list.iter().any(|(i, _, s)| i == &id && s.contains("Downloading")));

    // Remove it
    assert!(manager.remove(&id));
    let list = manager.list_downloads();
    assert!(!list.iter().any(|(i, _, _)| i == &id));

    // File cleanup checks
    assert!(!Path::new("downloads/meta").join(format!("{id}.meta.json")).exists());
    for i in 0..4 {
        assert!(!Path::new(&format!("{}.part{}", test_output_path(), i)).exists());
    }
}
