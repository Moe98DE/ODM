use std::fs::File;
use std::io::{self, Read, Write};
use crate::download::progress::SimpleProgressTracker;

pub fn write_stream_to_file<R: Read>(
    reader: &mut R,
    output_path: &str,
    tracker: &mut SimpleProgressTracker,
) -> io::Result<()> {
    let mut file = File::create(output_path)?;
    let mut buffer = [0; 8192];

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])?;
        tracker.update(n as u64);
    }

    Ok(())
}
