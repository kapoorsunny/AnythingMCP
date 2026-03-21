use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::error::Result;

pub fn run(tools_path: &Path, follow: bool, tail: usize) -> Result<()> {
    let log_path = crate::logger::log_path(tools_path);

    if !log_path.exists() {
        println!("No logs yet. Logs are created when you run mcpw commands.");
        return Ok(());
    }

    if follow {
        // Live tail — read existing then watch for new lines
        let file = std::fs::File::open(&log_path)?;
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().map_while(|l| l.ok()).collect();

        // Show last N lines first
        let start = if lines.len() > tail {
            lines.len() - tail
        } else {
            0
        };
        for line in &lines[start..] {
            println!("{}", line);
        }

        // Watch for new lines
        eprintln!("--- following {} (Ctrl+C to stop) ---", log_path.display());
        let mut last_size = std::fs::metadata(&log_path)?.len();

        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));

            let current_size = match std::fs::metadata(&log_path) {
                Ok(m) => m.len(),
                Err(_) => continue,
            };

            if current_size > last_size {
                let file = std::fs::File::open(&log_path)?;
                let reader = BufReader::new(file);
                let mut skipped = 0u64;
                for line in reader.lines() {
                    skipped += line.as_ref().map(|l| l.len() as u64 + 1).unwrap_or(0);
                    if skipped > last_size {
                        if let Ok(line) = line {
                            println!("{}", line);
                        }
                    }
                }
                last_size = current_size;
            }
        }
    } else {
        // Show last N lines
        let file = std::fs::File::open(&log_path)?;
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().map_while(|l| l.ok()).collect();

        let start = if lines.len() > tail {
            lines.len() - tail
        } else {
            0
        };

        for line in &lines[start..] {
            println!("{}", line);
        }
    }

    Ok(())
}
