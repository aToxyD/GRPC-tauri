//! Local file logging (daily file name) plus stdout; prunes old log files on startup.

use log::LevelFilter;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Resolved application log directory (read-only probes must not create it).
pub fn resolve_log_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("GRPC").join("logs"))
}

fn log_dir() -> Option<PathBuf> {
    resolve_log_dir()
}

fn parse_log_day(name: &str) -> Option<String> {
    name.strip_suffix(".log").map(str::to_string)
}

fn prune_logs_older_than_days(dir: &Path, keep_days: u64) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let cutoff = now.saturating_sub(keep_days * 86400);
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if parse_log_day(name).is_none() {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .map_err(|e| e.to_string())?
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        if modified < cutoff {
            let _ = fs::remove_file(&path);
        }
    }
    Ok(())
}

fn rust_log_level_filter() -> LevelFilter {
    match std::env::var("RUST_LOG").as_deref() {
        Ok("trace") | Ok("TRACE") => LevelFilter::Trace,
        Ok("debug") | Ok("DEBUG") => LevelFilter::Debug,
        Ok("warn") | Ok("WARN") => LevelFilter::Warn,
        Ok("error") | Ok("ERROR") => LevelFilter::Error,
        Ok("off") | Ok("OFF") => LevelFilter::Off,
        _ => LevelFilter::Info,
    }
}

/// Install `log` handlers: `logs/YYYY-MM-DD.log` under app data + stdout. Idempotent enough for tests.
pub fn init_file_logging() -> Result<(), String> {
    let base = log_dir().ok_or_else(|| "no data directory for logs".to_string())?;
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    prune_logs_older_than_days(&base, 30)?;

    let day = chrono::Utc::now().format("%Y-%m-%d");
    let log_path = base.join(format!("{}.log", day));
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("open log file {}: {}", log_path.display(), e))?;

    let level = rust_log_level_filter();

    fern::Dispatch::new()
        .level(level)
        .format(|out, message, record| {
            let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
            out.finish(format_args!(
                "[{}][{}][{}] {}",
                ts,
                record.level(),
                record.target(),
                message
            ))
        })
        .chain(std::io::stdout())
        .chain(log_file)
        .apply()
        .map_err(|e| format!("logger init: {}", e))?;

    log::info!(
        target: "grpc::startup",
        "logging initialized (file={}, level={:?})",
        log_path.display(),
        level
    );
    Ok(())
}
