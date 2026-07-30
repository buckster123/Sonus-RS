//! Error surface. Honesty invariants: degrades are stated ("no key
//! configured" beats a timeout), the key never appears in any message.

/// sunoapi.org's documented body-code table (docs/hermes-parity.md).
/// Note the traps: 429 = insufficient credits (NOT rate limit); 405 = rate
/// limit; 430 = call frequency too high.
pub fn code_meaning(code: i64) -> Option<&'static str> {
    Some(match code {
        400 => "invalid parameters",
        401 => "unauthorized — check SUNO_API_KEY",
        404 => "wrong path or method",
        405 => "rate limit exceeded (20 req / 10 s)",
        413 => "theme or prompt too long",
        429 => "insufficient credits",
        430 => "call frequency too high",
        455 => "system maintenance",
        500 => "server error",
        _ => return None,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum SonusError {
    #[error(
        "SUNO_API_KEY not configured — set it in /etc/sonus/env (node) or the environment (dev)"
    )]
    NotConfigured,
    /// Upstream said no inside the JSON body (`code != 200`).
    #[error("upstream error {code}: {msg}")]
    Api { code: i64, msg: String },
    /// Transport-level failure (connect, TLS, timeout, read).
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    /// A response we couldn't make sense of.
    #[error("unexpected response shape: {0}")]
    Shape(String),
}

impl SonusError {
    /// Build an Api error with the documented meaning folded into the
    /// message, so "429" reads as the credits problem it actually is.
    pub fn api(code: i64, msg: String) -> Self {
        let msg = match code_meaning(code) {
            Some(m) => format!("{msg} [{m}]"),
            None => msg,
        };
        SonusError::Api { code, msg }
    }

    /// Should a poll loop give up on this error immediately (true), or is it
    /// transient enough to ride out until the resumable timeout (false)?
    /// Transient: transport errors, garbage bodies, 5xx, rate/frequency
    /// limits (405/430), maintenance (455). Fatal: auth, bad request,
    /// missing route, insufficient credits.
    pub fn is_fatal(&self) -> bool {
        match self {
            SonusError::NotConfigured => true,
            SonusError::Api { code, .. } => {
                !(matches!(code, 405 | 430 | 455) || *code >= 500 || *code == 0)
            }
            SonusError::Http(_) | SonusError::Shape(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_errors_carry_the_documented_meaning() {
        let e = SonusError::api(429, "no".into());
        assert_eq!(
            e.to_string(),
            "upstream error 429: no [insufficient credits]"
        );
        let e = SonusError::api(418, "teapot".into());
        assert_eq!(e.to_string(), "upstream error 418: teapot");
    }

    #[test]
    fn fatality_split_matches_the_code_table() {
        for fatal in [400, 401, 404, 413, 429] {
            assert!(SonusError::api(fatal, String::new()).is_fatal(), "{fatal}");
        }
        for transient in [405, 430, 455, 500, 502, 0] {
            assert!(
                !SonusError::api(transient, String::new()).is_fatal(),
                "{transient}"
            );
        }
        assert!(!SonusError::Shape("garbage".into()).is_fatal());
        assert!(SonusError::NotConfigured.is_fatal());
    }
}
