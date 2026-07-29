//! Env-driven configuration (the parity doc's env contract).

use std::path::PathBuf;

pub const DEFAULT_API_BASE: &str = "https://api.sunoapi.org/api/v1";

#[derive(Debug, Clone)]
pub struct Config {
    /// Upstream base (SUNO_API_BASE, then SUNO_BASE_URL, then the default).
    pub api_base: String,
    /// The money key (SUNO_API_KEY). None = honest "not configured" errors.
    pub api_key: Option<String>,
    /// Where tracks land (SUNO_DOWNLOAD_DIR; agentd points this at
    /// workspace/sonus so the player + SCORE picker see them).
    pub download_dir: PathBuf,
}

impl Config {
    /// Resolve from a key→value lookup (tests inject; `from_env` wraps).
    pub fn resolve(get: impl Fn(&str) -> Option<String>) -> Self {
        let api_base = get("SUNO_API_BASE")
            .or_else(|| get("SUNO_BASE_URL"))
            .map(|s| normalize_base(&s))
            .unwrap_or_else(|| DEFAULT_API_BASE.to_string());
        let api_key = get("SUNO_API_KEY")
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());
        let download_dir = get("SUNO_DOWNLOAD_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs_fallback().join("sonus"));
        Self {
            api_base,
            api_key,
            download_dir,
        }
    }

    pub fn from_env() -> Self {
        Self::resolve(|k| std::env::var(k).ok())
    }
}

/// Trim + drop a trailing slash; a bare host gets the /api/v1 suffix so both
/// spellings of the env var work (hermes accepted either).
fn normalize_base(raw: &str) -> String {
    let t = raw.trim().trim_end_matches('/');
    if t.ends_with("/api/v1") {
        t.to_string()
    } else {
        format!("{t}/api/v1")
    }
}

fn dirs_fallback() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".local/share")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(vars: &[(&str, &str)]) -> Config {
        Config::resolve(|k| {
            vars.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        })
    }

    #[test]
    fn defaults_are_the_parity_contract() {
        let c = cfg(&[]);
        assert_eq!(c.api_base, "https://api.sunoapi.org/api/v1");
        assert!(c.api_key.is_none());
        assert!(c.download_dir.ends_with("sonus"));
    }

    #[test]
    fn base_normalizes_both_env_spellings() {
        assert_eq!(
            cfg(&[("SUNO_API_BASE", "https://x.test/api/v1/")]).api_base,
            "https://x.test/api/v1"
        );
        assert_eq!(
            cfg(&[("SUNO_BASE_URL", "https://x.test")]).api_base,
            "https://x.test/api/v1"
        );
        // SUNO_API_BASE wins over SUNO_BASE_URL (hermes precedence)
        let c = cfg(&[
            ("SUNO_API_BASE", "https://a.test"),
            ("SUNO_BASE_URL", "https://b.test"),
        ]);
        assert_eq!(c.api_base, "https://a.test/api/v1");
    }

    #[test]
    fn blank_key_is_honestly_absent() {
        assert!(cfg(&[("SUNO_API_KEY", "  ")]).api_key.is_none());
        assert_eq!(
            cfg(&[("SUNO_API_KEY", " k123 ")]).api_key.as_deref(),
            Some("k123")
        );
    }
}
