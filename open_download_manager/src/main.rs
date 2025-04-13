mod download;
mod config;
mod state;
pub mod core;

use config::Config;

fn main() {
    let config = Config::load_from_file("config.toml");

    let url = "https://proof.ovh.net/files/100Mb.dat";
    //let output = format!("{}test.bin", config.default_output_path);
    let output = format!("test.bin");

    match download::manager::download_file_segmented(&url, &output, config.num_threads, &config) {
        Ok(_) => println!("✅ Done!"),
        Err(e) => eprintln!("❌ Error: {}", e),
    }
}
