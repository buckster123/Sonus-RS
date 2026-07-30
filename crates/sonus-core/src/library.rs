//! library — downloads into SUNO_DOWNLOAD_DIR: honest names, dedupe,
//! disk guard (S2).
//!
//! Filename convention is hermes' MCP `download_track` one (the shape the
//! deployed plugin produced): `{task_id[..12]}__{index}__{safe_title}{ext}`,
//! index 1-based. Downloads stream to `{name}.part` then rename — a partial
//! download never masquerades as a finished track.
//!
//! SECURITY: the Library client is keyless ON PURPOSE. Audio lives on an
//! unauthenticated third-party relay CDN (tempfile.aiquickdraw.com in the
//! field) — the API key must never travel there.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::AsyncWriteExt;

use crate::config::Config;
use crate::error::SonusError;
use crate::types::Track;

/// Headroom kept free beyond an announced Content-Length.
const DISK_MARGIN: u64 = 16 * 1024 * 1024;
/// Free-space floor demanded when upstream announces no length.
const UNKNOWN_SIZE_FLOOR: u64 = 64 * 1024 * 1024;
const MB: u64 = 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Whole-body budget — CDN files are a few MB; 10 min covers a very slow Pi
/// link without ever hanging forever.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

/// One track landed (or found already landed) on disk.
#[derive(Debug, Clone)]
pub struct TrackFile {
    pub file: PathBuf,
    pub bytes: u64,
    /// Dedupe hit: the file already existed with content — no bytes moved.
    pub skipped_existing: bool,
    pub track_id: Option<String>,
    pub title: String,
}

/// The honest whole-batch story: what landed, what failed, what upstream
/// never gave us a URL for. Nothing is silently dropped.
#[derive(Debug, Default)]
pub struct DownloadReport {
    pub files: Vec<TrackFile>,
    /// Per-track download failures ("track 2 (Title): …"); the batch
    /// continues past them (one bad CDN fetch doesn't kill its sibling).
    pub failures: Vec<String>,
    /// Tracks upstream reported without any resolvable audio URL.
    pub undownloadable: Vec<String>,
}

pub struct Library {
    http: reqwest::Client,
    dir: PathBuf,
}

impl Library {
    /// Keyless by design — see the module SECURITY note. Works without
    /// SUNO_API_KEY: downloading from known URLs needs no credentials.
    pub fn new(cfg: &Config) -> Result<Self, SonusError> {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(DOWNLOAD_TIMEOUT)
            .build()?;
        Ok(Self {
            http,
            dir: cfg.download_dir.clone(),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Download every track of a task. Individual failures are collected,
    /// not fatal; a tripped disk guard aborts the batch (node health).
    pub async fn download_tracks(
        &self,
        task_id: &str,
        tracks: &[Track],
    ) -> Result<DownloadReport, SonusError> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let mut report = DownloadReport::default();
        for (i, track) in tracks.iter().enumerate() {
            let index = i + 1; // hermes filenames are 1-based
            let Some(url) = track.audio_url.clone() else {
                report.undownloadable.push(format!(
                    "track {index} ({}) has no downloadable URL",
                    label(track, index)
                ));
                continue;
            };
            match self.download_one(task_id, index, track, &url).await {
                Ok(f) => report.files.push(f),
                Err(e @ SonusError::DiskFull { .. }) => return Err(e),
                Err(e) => report
                    .failures
                    .push(format!("track {index} ({}): {e}", label(track, index))),
            }
        }
        Ok(report)
    }

    async fn download_one(
        &self,
        task_id: &str,
        index: usize,
        track: &Track,
        url: &str,
    ) -> Result<TrackFile, SonusError> {
        let name = track_filename(task_id, index, &track.title, url);
        let path = self.dir.join(&name);
        if let Ok(meta) = tokio::fs::metadata(&path).await {
            if meta.len() > 0 {
                return Ok(TrackFile {
                    file: path,
                    bytes: meta.len(),
                    skipped_existing: true,
                    track_id: track.id.clone(),
                    title: track.title.clone(),
                });
            }
        }
        // no bearer_auth here — third-party CDN, see module SECURITY note
        let mut resp = self.http.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(SonusError::Shape(format!(
                "download HTTP {} for {name}",
                resp.status().as_u16()
            )));
        }
        let needed = required_bytes(resp.content_length());
        let available = available_bytes(&self.dir)?;
        if available < needed {
            return Err(SonusError::DiskFull {
                needed_mb: needed / MB,
                available_mb: available / MB,
                dir: self.dir.display().to_string(),
            });
        }
        let part = self.dir.join(format!("{name}.part"));
        let bytes = match stream_to(&part, &mut resp).await {
            Ok(0) => {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(SonusError::Shape(format!("empty download for {name}")));
            }
            Ok(n) => n,
            Err(e) => {
                // never leave a lying .part behind
                let _ = tokio::fs::remove_file(&part).await;
                return Err(e);
            }
        };
        tokio::fs::rename(&part, &path).await?;
        Ok(TrackFile {
            file: path,
            bytes,
            skipped_existing: false,
            track_id: track.id.clone(),
            title: track.title.clone(),
        })
    }
}

async fn stream_to(part: &Path, resp: &mut reqwest::Response) -> Result<u64, SonusError> {
    let mut file = tokio::fs::File::create(part).await?;
    let mut bytes = 0u64;
    while let Some(chunk) = resp.chunk().await? {
        bytes += chunk.len() as u64;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(bytes)
}

fn label(track: &Track, index: usize) -> String {
    if !track.title.is_empty() {
        track.title.clone()
    } else {
        track.id.clone().unwrap_or_else(|| format!("track_{index}"))
    }
}

/// hermes' safe_title: keep alphanumerics/space/-/_, everything else → `_`;
/// truncate to 60 chars, trim, spaces → `_`; empty → `track_{index}`.
pub fn safe_title(title: &str, index: usize) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    let cleaned = cleaned.trim().replace(' ', "_");
    if cleaned.is_empty() {
        format!("track_{index}")
    } else {
        cleaned
    }
}

/// hermes' extension rule: first of .mp3/.wav/.ogg/.m4a appearing ANYWHERE
/// in the lowercased URL (substring, survives query strings); default .mp3.
pub fn extension_for(url: &str) -> &'static str {
    let lower = url.to_lowercase();
    for ext in [".mp3", ".wav", ".ogg", ".m4a"] {
        if lower.contains(ext) {
            return ext;
        }
    }
    ".mp3"
}

/// `{task_id[..12]}__{index}__{safe_title}{ext}` — the deployed plugin's
/// on-disk shape, so a cutover node's library stays visually consistent.
pub fn track_filename(task_id: &str, index: usize, title: &str, url: &str) -> String {
    let tid: String = task_id.chars().take(12).collect();
    format!(
        "{tid}__{index}__{}{}",
        safe_title(title, index),
        extension_for(url)
    )
}

/// Space the guard demands before it lets a download start.
pub fn required_bytes(content_length: Option<u64>) -> u64 {
    match content_length {
        Some(n) => n + DISK_MARGIN,
        None => UNKNOWN_SIZE_FLOOR,
    }
}

#[cfg(unix)]
fn available_bytes(dir: &Path) -> Result<u64, SonusError> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(dir.as_os_str().as_bytes())
        .map_err(|e| SonusError::Shape(format!("bad download dir path: {e}")))?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut s) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(s.f_bavail as u64 * s.f_frsize as u64)
}

#[cfg(not(unix))]
fn available_bytes(_dir: &Path) -> Result<u64, SonusError> {
    Ok(u64::MAX) // nodes are unix; dev elsewhere skips the guard honestly
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_title_is_hermes_parity() {
        assert_eq!(
            safe_title("Trismegistus Fanfare", 1),
            "Trismegistus_Fanfare"
        );
        assert_eq!(safe_title("A/B: test!", 1), "A_B__test_");
        assert_eq!(safe_title("  ", 3), "track_3");
        assert_eq!(safe_title("", 2), "track_2");
        // unicode letters survive (python isalnum parity)
        assert_eq!(safe_title("Björk Løp", 1), "Björk_Løp");
        // truncation happens before trim/replace, at 60 chars
        let long = "x".repeat(80);
        assert_eq!(safe_title(&long, 1).len(), 60);
    }

    #[test]
    fn extension_scan_is_substring_and_ordered() {
        assert_eq!(extension_for("https://c.dn/r/abc.mp3"), ".mp3");
        assert_eq!(extension_for("https://c.dn/r/abc.MP3?sig=x"), ".mp3");
        assert_eq!(extension_for("https://c.dn/r/abc.wav"), ".wav");
        // .mp3 wins over .wav because the scan order says so (hermes parity)
        assert_eq!(extension_for("https://c.dn/x.wav/y.mp3"), ".mp3");
        assert_eq!(extension_for("https://c.dn/stream/abc"), ".mp3");
    }

    #[test]
    fn filename_matches_the_deployed_plugin_shape() {
        assert_eq!(
            track_filename(
                "ae2ad3f9fabcdee05de4deca2e521d9d",
                1,
                "Trismegistus Fanfare",
                "https://tempfile.aiquickdraw.com/r/e219448d8f2d41d491a321766c7e38bd.mp3"
            ),
            "ae2ad3f9fabc__1__Trismegistus_Fanfare.mp3"
        );
        // short task ids don't panic
        assert_eq!(track_filename("abc", 2, "T", "u.wav"), "abc__2__T.wav");
    }

    #[test]
    fn disk_guard_demands_margin_or_floor() {
        assert_eq!(required_bytes(Some(5 * MB)), 5 * MB + DISK_MARGIN);
        assert_eq!(required_bytes(None), UNKNOWN_SIZE_FLOOR);
    }

    #[test]
    fn available_bytes_reads_a_real_filesystem() {
        let n = available_bytes(&std::env::temp_dir()).unwrap();
        assert!(n > 0, "temp dir should have some space");
    }
}
