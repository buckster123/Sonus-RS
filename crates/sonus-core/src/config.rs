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

    /// Process env first, then the node env file (`SONUS_ENV_FILE`, default
    /// `/etc/sonus/env`). This is HOW key isolation works without a daemon:
    /// there's no systemd EnvironmentFile reader here — the MCP child (agentd
    /// user) self-loads the root:agentd 0640 file, so the key never has to
    /// live in /etc/agentd/env or plugins.toml. An empty process var doesn't
    /// shadow the file (the "add the key later" UX stays one-file).
    pub fn from_env() -> Self {
        let path = std::env::var("SONUS_ENV_FILE")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_ENV_FILE.to_string());
        let file_vars = std::fs::read_to_string(&path)
            .map(|s| parse_env_file(&s))
            .unwrap_or_default();
        Self::resolve(|k| {
            std::env::var(k)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or_else(|| file_vars.get(k).cloned())
        })
    }
}

pub const DEFAULT_ENV_FILE: &str = "/etc/sonus/env";

/// KEY=VALUE lines; `#` comments and blanks skipped; whitespace trimmed;
/// one layer of matching quotes stripped. No interpolation — it's an env
/// file, not a shell script.
fn parse_env_file(raw: &str) -> std::collections::HashMap<String, String> {
    raw.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (k, v) = line.split_once('=')?;
            let k = k.trim();
            let mut v = v.trim();
            if v.len() >= 2
                && ((v.starts_with('"') && v.ends_with('"'))
                    || (v.starts_with('\'') && v.ends_with('\'')))
            {
                v = &v[1..v.len() - 1];
            }
            (!k.is_empty()).then(|| (k.to_string(), v.to_string()))
        })
        .collect()
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

    #[test]
    fn env_file_parses_the_node_format() {
        let vars = parse_env_file(
            "# Sonus node env — the Suno key lives HERE and only here.\n\
             SUNO_API_KEY=sk-abc123\n\
             \n\
             QUOTED=\"with spaces\"\n\
             SINGLE='also fine'\n\
             SPACED = padded \n\
             =novalue\n\
             not a kv line\n",
        );
        assert_eq!(
            vars.get("SUNO_API_KEY").map(String::as_str),
            Some("sk-abc123")
        );
        assert_eq!(vars.get("QUOTED").map(String::as_str), Some("with spaces"));
        assert_eq!(vars.get("SINGLE").map(String::as_str), Some("also fine"));
        assert_eq!(vars.get("SPACED").map(String::as_str), Some("padded"));
        assert!(!vars.contains_key(""));
        assert_eq!(vars.len(), 4);
    }

    #[test]
    fn from_env_falls_back_to_the_file_but_process_wins() {
        let dir = std::env::temp_dir().join(format!("sonus-env-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("env");
        std::fs::write(
            &file,
            "SUNO_API_KEY=file-key\nSUNO_API_BASE=https://file.test\n",
        )
        .unwrap();

        // isolate: point at our file, control the process env for this test
        std::env::set_var("SONUS_ENV_FILE", &file);
        std::env::remove_var("SUNO_API_KEY");
        std::env::set_var("SUNO_API_BASE", "https://process.test");

        let c = Config::from_env();
        assert_eq!(c.api_key.as_deref(), Some("file-key"), "file fills the gap");
        assert_eq!(
            c.api_base, "https://process.test/api/v1",
            "process env wins over the file"
        );

        // an EMPTY process var must not shadow the file
        std::env::set_var("SUNO_API_KEY", "");
        let c = Config::from_env();
        assert_eq!(c.api_key.as_deref(), Some("file-key"));

        std::env::remove_var("SONUS_ENV_FILE");
        std::env::remove_var("SUNO_API_BASE");
        std::env::remove_var("SUNO_API_KEY");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
