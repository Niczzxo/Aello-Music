//! Minimal diagnostics log: one append-only text file in the app data
//! dir (`aello-diagnostics.log`) so a user can send the actual error
//! when sign-in or playback fails on their PC. No log framework, no
//! rotation beyond a size cap — the file is truncated to the last
//! ~2000 lines when it grows past 512 KiB. Best-effort everywhere:
//! logging must never fail the operation it describes. No cookies,
//! tokens, or stream URLs are ever written here — only the failing
//! step and its reason.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const MAX_BYTES: u64 = 512 * 1024;
const KEEP_LINES: usize = 2000;

/// `aello-diagnostics.log` next to settings.json / auth.json.
pub fn log_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("aello-diagnostics.log"))
        .unwrap_or_else(|_| std::env::temp_dir().join("aello-diagnostics.log"))
}

fn timestamp() -> String {
    // Seconds since epoch is enough to order entries; the app also
    // stamps its own wall-clock time in the message where it matters.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}

/// Append one `[epoch] [kind] message` line. Swallows all errors.
pub fn append_log(app: &AppHandle, kind: &str, message: &str) {
    let path = log_path(app);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Truncate an overgrown file to the last KEEP_LINES lines first.
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > MAX_BYTES {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let lines: Vec<&str> = text.lines().collect();
                if lines.len() > KEEP_LINES {
                    let keep = lines[lines.len() - KEEP_LINES..].join("\n");
                    let _ = std::fs::write(&path, keep);
                }
            }
        }
    }
    let line = format!(
        "[{}] [{}] {}\n",
        timestamp(),
        kind,
        message.replace('\n', " ")
    );
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(line.as_bytes());
    }
}

/// Last `max_lines` lines of the log (empty when there is no log yet).
pub fn read_tail(app: &AppHandle, max_lines: usize) -> String {
    let Ok(text) = std::fs::read_to_string(log_path(app)) else {
        return String::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines {
        return text;
    }
    lines[lines.len() - max_lines..].join("\n")
}
