use crate::download::file::write_stream_to_file;
use crate::download::progress::SimpleProgressTracker;
use reqwest::blocking::Client;
use reqwest::header::USER_AGENT;

pub fn download_file(url: &str, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let mut response = client
        .get(url)
        .header(USER_AGENT, "OpenDownloadManager/0.1")
        .send()?
        .error_for_status()?;

    let total_size = response
        .content_length()
        .ok_or("Couldn't get content length")?;

    let mut tracker = SimpleProgressTracker::new(total_size);
    write_stream_to_file(&mut response, output_path, &mut tracker)?;

    println!("\n✅ Download complete: {}", output_path);
    Ok(())
}
