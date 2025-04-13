use std::fs;
use std::path::Path;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub timeout_secs: u64,
    pub max_retries: u8,
    pub num_threads: usize,
    pub default_output_path: String,
}


impl Default for Config {
    fn default() -> Self {
        Self {
            timeout_secs: 15,
            max_retries: 3,
            num_threads: 4,
            default_output_path: "".to_string(),
            //default_output_path: "/Volumes/WD ELEMENTS/ODM/ODM/open_download_manager/".to_string(),
        }
    }
}

impl Config {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Self {
        let contents = fs::read_to_string(path);
        match contents {
            Ok(toml_str) => toml::from_str(&toml_str).unwrap_or_else(|err| {
                eprintln!("⚠️ Failed to parse config.toml: {} — using defaults", err);
                Config::default()
            }),
            Err(_) => {
                println!("⚠️ config.toml not found — using defaults");
                Config::default()
            }
        }
    }
}
