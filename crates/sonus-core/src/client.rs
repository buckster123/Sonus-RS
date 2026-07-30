//! The Bearer HTTP client: generate / record-info / credit, plus the
//! resumable poll loop.
//!
//! Design notes (docs/hermes-parity.md § Rust-side divergences):
//! - No User-Agent spoof: the plain-requests path (Bearer only) is the one
//!   real hermes generations succeeded through.
//! - Error bodies are read as envelopes regardless of HTTP status — upstream
//!   puts truth in `code`/`msg`, not the status line.
//! - A poll timeout is an OUTCOME, not an error: the task id stays valid
//!   upstream (15-day retention) and the caller can re-poll any time.

use std::time::{Duration, Instant};

use serde_json::Value;

use crate::config::Config;
use crate::error::SonusError;
use crate::types::{self, Credits, GenerateParams, RecordInfo};

pub const INITIAL_POLL_INTERVAL: Duration = Duration::from_secs(5);
pub const MAX_POLL_INTERVAL: Duration = Duration::from_secs(30);
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Poll backoff: 5s → ×1.5 → capped at 30s (hermes' schedule).
pub fn next_interval(cur: Duration) -> Duration {
    cur.mul_f64(1.5).min(MAX_POLL_INTERVAL)
}

/// What a bounded poll produced. Timing out is not failure — the paid task
/// is still live upstream; resume by polling the same task id.
#[derive(Debug)]
pub enum PollOutcome {
    /// Terminal state reached — inspect `RecordInfo.status` for
    /// Success vs Failed.
    Done(RecordInfo),
    /// Budget exhausted while the task was still in flight.
    TimedOut {
        last: Option<RecordInfo>,
        waited: Duration,
    },
}

pub struct SunoClient {
    http: reqwest::Client,
    base: String,
    key: String,
    initial_poll: Duration,
}

impl SunoClient {
    /// Fails honestly when the key is absent — no client, no accidental
    /// unauthenticated calls.
    pub fn new(cfg: &Config) -> Result<Self, SonusError> {
        let key = cfg.api_key.clone().ok_or(SonusError::NotConfigured)?;
        let http = reqwest::Client::builder().timeout(HTTP_TIMEOUT).build()?;
        Ok(Self {
            http,
            base: cfg.api_base.clone(),
            key,
            initial_poll: INITIAL_POLL_INTERVAL,
        })
    }

    /// Test knob: shrink the first poll interval so loops run in test time.
    pub fn with_initial_poll_interval(mut self, d: Duration) -> Self {
        self.initial_poll = d;
        self
    }

    /// Submit a generation. SPENDS CREDITS — callers gate this explicitly
    /// (check_credits first; nothing in this crate auto-fires it).
    pub async fn generate(&self, params: &GenerateParams) -> Result<String, SonusError> {
        let (_, v) = self
            .send_json(
                self.http
                    .post(format!("{}/generate", self.base))
                    .json(&params.body()),
            )
            .await?;
        types::parse_task_id(&v)
    }

    /// One lifecycle read. Free.
    pub async fn record_info(&self, task_id: &str) -> Result<RecordInfo, SonusError> {
        let (_, v) = self
            .send_json(
                self.http
                    .get(format!("{}/generate/record-info", self.base))
                    .query(&[("taskId", task_id)]),
            )
            .await?;
        types::parse_record_info(&v)
    }

    /// Remaining credits. Free — the spend gate.
    pub async fn credits(&self) -> Result<Credits, SonusError> {
        let resp = self
            .http
            .get(format!("{}/generate/credit", self.base))
            .bearer_auth(&self.key)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if status == 404 {
            // some instances don't expose the endpoint — honest unknown
            return Ok(Credits::Unknown);
        }
        let v = parse_body(status, &resp.text().await?)?;
        types::parse_credits(status, &v)
    }

    /// Poll until terminal or `timeout`. Transient errors (transport, 5xx,
    /// rate limits, maintenance) ride out inside the budget; fatal ones
    /// (auth, bad task id, insufficient credits) surface immediately.
    pub async fn poll_until_done(
        &self,
        task_id: &str,
        timeout: Duration,
    ) -> Result<PollOutcome, SonusError> {
        let start = Instant::now();
        let mut interval = self.initial_poll;
        let mut last: Option<RecordInfo> = None;
        loop {
            match self.record_info(task_id).await {
                Ok(info) => {
                    if info.status.is_terminal() {
                        return Ok(PollOutcome::Done(info));
                    }
                    last = Some(info);
                }
                Err(e) if e.is_fatal() => return Err(e),
                Err(e) => {
                    tracing::warn!(task_id, "transient poll error, will retry: {e}");
                }
            }
            if start.elapsed() + interval > timeout {
                return Ok(PollOutcome::TimedOut {
                    last,
                    waited: start.elapsed(),
                });
            }
            tokio::time::sleep(interval).await;
            interval = next_interval(interval);
        }
    }

    async fn send_json(&self, req: reqwest::RequestBuilder) -> Result<(u16, Value), SonusError> {
        let resp = req.bearer_auth(&self.key).send().await?;
        let status = resp.status().as_u16();
        let text = resp.text().await?;
        Ok((status, parse_body(status, &text)?))
    }
}

fn parse_body(status: u16, text: &str) -> Result<Value, SonusError> {
    serde_json::from_str(text).map_err(|_| {
        SonusError::Shape(format!(
            "HTTP {status}: non-JSON response ({} bytes)",
            text.len()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_schedule_is_hermes_parity() {
        let mut d = INITIAL_POLL_INTERVAL;
        let mut seen = vec![d.as_secs_f64()];
        for _ in 0..5 {
            d = next_interval(d);
            seen.push(d.as_secs_f64());
        }
        assert_eq!(seen, vec![5.0, 7.5, 11.25, 16.875, 25.3125, 30.0]);
        // and it stays capped
        assert_eq!(next_interval(d), MAX_POLL_INTERVAL);
    }

    #[test]
    fn no_key_no_client() {
        let cfg = Config::resolve(|_| None);
        assert!(matches!(
            SunoClient::new(&cfg),
            Err(SonusError::NotConfigured)
        ));
    }
}
