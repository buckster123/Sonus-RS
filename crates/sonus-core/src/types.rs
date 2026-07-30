//! Wire types + pure parsers for the sunoapi.org contract.
//!
//! Field truth: docs/hermes-parity.md, extracted from hermes-sonus-v2 and a
//! real captured V5 run. Deliberate divergences from the Python are tagged
//! FIX in comments and listed in the parity doc — the headline one: upstream
//! failure statuses terminate the lifecycle here, they don't poll to timeout.

use serde_json::{json, Map, Value};

use crate::error::SonusError;

/// Model names as they appear on the wire (underscore forms — `V5_5`, never
/// `V5.5`). Sent raw; upstream owns validation.
pub const MODELS: &[&str] = &["V4", "V4_5", "V4_5PLUS", "V4_5ALL", "V5", "V5_5"];

/// Upstream requires `callBackUrl` by schema but never checks reachability —
/// hermes shipped this exact literal on every real successful generation.
/// v1 is poll-only (no callback server), so we do the same.
pub const NULL_CALLBACK_URL: &str = "https://localhost/callback";

/// A `POST /generate` request. `body()` reproduces hermes' build_payload
/// field rules exactly (mode-dependent presence, camelCase keys).
#[derive(Debug, Clone, Default)]
pub struct GenerateParams {
    /// false = "simple" mode: upstream invents lyrics/style from `prompt`.
    pub custom_mode: bool,
    pub instrumental: bool,
    pub model: String,
    /// Style/genre CSV — reaches the wire in custom mode only.
    pub style: Option<String>,
    pub title: Option<String>,
    /// Custom mode: the lyrics. Simple mode: the whole song description.
    pub prompt: Option<String>,
    /// Styles to avoid (`negativeTags`).
    pub negative_tags: Option<String>,
    /// 0.0–1.0 → `weirdnessConstraint`, rounded to 2 decimals.
    pub weirdness: Option<f64>,
    /// 0.0–1.0 → `styleWeight`, rounded to 2 decimals.
    pub style_weight: Option<f64>,
    /// "m" | "f"; only sent when not instrumental.
    pub vocal_gender: Option<String>,
    /// None → [`NULL_CALLBACK_URL`].
    pub callback_url: Option<String>,
}

impl GenerateParams {
    pub fn body(&self) -> Value {
        let mut b = Map::new();
        b.insert("customMode".into(), json!(self.custom_mode));
        b.insert("instrumental".into(), json!(self.instrumental));
        b.insert("model".into(), json!(self.model));
        b.insert(
            "callBackUrl".into(),
            json!(self
                .callback_url
                .clone()
                .unwrap_or_else(|| NULL_CALLBACK_URL.to_string())),
        );
        if self.custom_mode {
            if let Some(s) = nonempty(&self.style) {
                b.insert("style".into(), json!(s));
            }
            // hermes parity: custom mode ALWAYS sends title, "" when unset
            b.insert(
                "title".into(),
                json!(self.title.clone().unwrap_or_default()),
            );
            // hermes parity: lyrics ride along even when instrumental
            // (deliberate, commented deviation from Suno's own docs)
            if let Some(p) = nonempty(&self.prompt) {
                b.insert("prompt".into(), json!(p));
            }
            if let Some(n) = nonempty(&self.negative_tags) {
                b.insert("negativeTags".into(), json!(n));
            }
        } else {
            // simple mode sends ONLY prompt beside the four base fields
            let p = nonempty(&self.prompt)
                .or_else(|| nonempty(&self.style))
                .unwrap_or_default();
            b.insert("prompt".into(), json!(p.trim()));
        }
        if let Some(w) = self.weirdness {
            b.insert("weirdnessConstraint".into(), json!(round2(w)));
        }
        if let Some(w) = self.style_weight {
            b.insert("styleWeight".into(), json!(round2(w)));
        }
        if !self.instrumental {
            if let Some(g) = nonempty(&self.vocal_gender) {
                b.insert("vocalGender".into(), json!(g.to_lowercase()));
            }
        }
        Value::Object(b)
    }
}

/// A `POST /generate/extend` request (hermes' exact body: audioId/taskId/
/// model/callBackUrl/continueAt always; prompt/style/title when non-empty).
#[derive(Debug, Clone, Default)]
pub struct ExtendParams {
    /// The original generation task.
    pub task_id: String,
    /// The specific variant to extend (each task yields 2).
    pub audio_id: String,
    pub model: String,
    /// Seconds into the original to extend from; 0 = continue from end.
    pub continue_at: i64,
    pub prompt: Option<String>,
    pub style: Option<String>,
    pub title: Option<String>,
    pub callback_url: Option<String>,
}

impl ExtendParams {
    pub fn body(&self) -> Value {
        let mut b = Map::new();
        b.insert("audioId".into(), json!(self.audio_id));
        b.insert("taskId".into(), json!(self.task_id));
        b.insert("model".into(), json!(self.model));
        b.insert(
            "callBackUrl".into(),
            json!(self
                .callback_url
                .clone()
                .unwrap_or_else(|| NULL_CALLBACK_URL.to_string())),
        );
        b.insert("continueAt".into(), json!(self.continue_at));
        for (k, v) in [
            ("prompt", &self.prompt),
            ("style", &self.style),
            ("title", &self.title),
        ] {
            if let Some(s) = nonempty(v) {
                b.insert(k.into(), json!(s));
            }
        }
        Value::Object(b)
    }
}

/// `POST /lyrics` body — exactly two fields (hermes parity).
pub fn lyrics_body(prompt: &str, callback_url: Option<&str>) -> Value {
    json!({
        "prompt": prompt,
        "callBackUrl": callback_url.unwrap_or(NULL_CALLBACK_URL),
    })
}

fn nonempty(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from)
}

fn round2(x: f64) -> f64 {
    (x.clamp(0.0, 1.0) * 100.0).round() / 100.0
}

/// The task lifecycle as upstream reports it (statuses normalized lowercase).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    /// Lyrics/text stage done; audio still rendering.
    TextSuccess,
    /// First of the two variants ready; the second is still rendering.
    FirstSuccess,
    /// Fully complete — all tracks present.
    Success,
    /// Terminal failure; carries the normalized upstream status token
    /// (e.g. "sensitive_word_error"). FIX over hermes: the four real Suno
    /// failure statuses land here instead of polling until timeout.
    Failed(String),
    /// A status token we don't know — non-terminal, keep polling (the
    /// resumable timeout bounds it).
    Unknown(String),
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, TaskStatus::Success | TaskStatus::Failed(_))
    }

    /// The lowercase status token the MCP surface reports (Failed/Unknown
    /// carry their upstream token through unchanged).
    pub fn as_token(&self) -> &str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::TextSuccess => "text_success",
            TaskStatus::FirstSuccess => "first_success",
            TaskStatus::Success => "success",
            TaskStatus::Failed(s) | TaskStatus::Unknown(s) => s,
        }
    }
}

const FAILURE_STATUSES: &[&str] = &[
    // documented sunoapi.org failure statuses (hermes never handled these)
    "create_task_failed",
    "generate_audio_failed",
    "callback_exception",
    "sensitive_word_error",
    // hermes' own terminal-failure tokens
    "error",
    "failed",
    "cancelled",
    "expired",
];

/// One generated track. Suno returns two variants per generation.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub id: Option<String>,
    pub title: String,
    /// Resolved by the field-proven preference order (sourceAudioUrl first).
    /// None = present-but-undownloadable; S2 reports these honestly instead
    /// of dropping them silently like the Python did.
    pub audio_url: Option<String>,
    pub image_url: Option<String>,
    /// Seconds; upstream sends floats (168.6).
    pub duration: Option<f64>,
    pub tags: Option<String>,
}

/// A `GET /generate/record-info` snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordInfo {
    pub task_id: Option<String>,
    pub status: TaskStatus,
    pub tracks: Vec<Track>,
    /// Upstream's human-facing failure detail when present.
    pub error_message: Option<String>,
}

/// Remaining credits (`GET /generate/credit` — free, the spend gate).
#[derive(Debug, Clone, PartialEq)]
pub enum Credits {
    Known {
        remaining: f64,
        total: Option<f64>,
    },
    /// The endpoint 404s on some sunoapi.org instances — honest unknown.
    Unknown,
}

/// `POST /generate` response → task id. Success gate is `code == 200` inside
/// the body plus a truthy `data.taskId` (snake `task_id` accepted as
/// fallback, hermes batch parity).
pub fn parse_task_id(v: &Value) -> Result<String, SonusError> {
    let code = envelope_code(v);
    if code != 200 {
        return Err(SonusError::api(code, envelope_msg(v)));
    }
    v.get("data")
        .and_then(|d| d.get("taskId").or_else(|| d.get("task_id")))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or_else(|| SonusError::Shape(format!("no taskId in response: {}", envelope_msg(v))))
}

/// `GET /generate/record-info` response → typed snapshot.
pub fn parse_record_info(v: &Value) -> Result<RecordInfo, SonusError> {
    let code = envelope_code(v);
    if code != 200 {
        return Err(SonusError::api(code, envelope_msg(v)));
    }
    let data = v.get("data").unwrap_or(&Value::Null);
    let tracks = extract_tracks(data);
    let status = match raw_status(data) {
        Some(s) => classify_status(&s),
        // no status token but tracks present → complete (the fallback real
        // hermes runs actually completed through)
        None if !tracks.is_empty() => TaskStatus::Success,
        None => TaskStatus::Pending,
    };
    let task_id = data
        .get("taskId")
        .or_else(|| data.get("task_id"))
        .and_then(Value::as_str)
        .map(String::from);
    let error_message = ["errorMessage", "errorCode"]
        .iter()
        .find_map(|k| {
            data.get(k)
                .or_else(|| data.get("response").and_then(|r| r.get(k)))
                .and_then(Value::as_str)
        })
        .map(String::from)
        .or_else(|| {
            if matches!(status, TaskStatus::Failed(_)) {
                v.get("msg").and_then(Value::as_str).map(String::from)
            } else {
                None
            }
        });
    Ok(RecordInfo {
        task_id,
        status,
        tracks,
        error_message,
    })
}

/// `GET /generate/credit` → typed credits. `http_status` matters: a 404
/// means "this instance doesn't expose the endpoint", not an error.
pub fn parse_credits(http_status: u16, v: &Value) -> Result<Credits, SonusError> {
    if http_status == 404 {
        return Ok(Credits::Unknown);
    }
    let code = envelope_code(v);
    if code == 404 {
        return Ok(Credits::Unknown);
    }
    if code != 200 {
        return Err(SonusError::api(code, envelope_msg(v)));
    }
    match v.get("data") {
        // the documented sunoapi.org shape: data is a bare number
        Some(n) if n.is_number() => Ok(Credits::Known {
            remaining: n.as_f64().unwrap_or(0.0),
            total: None,
        }),
        Some(Value::Object(o)) => Ok(Credits::Known {
            remaining: o.get("remaining").and_then(Value::as_f64).unwrap_or(0.0),
            total: o.get("total").and_then(Value::as_f64),
        }),
        other => Err(SonusError::Shape(format!(
            "unrecognized credit payload: {}",
            other.cloned().unwrap_or(Value::Null)
        ))),
    }
}

/// Status token extraction, hermes' exact algorithm: `status` →
/// `callbackType` → `state` on data, then the same three inside
/// `data.response`; lowercased + trimmed.
fn raw_status(data: &Value) -> Option<String> {
    let scopes = [Some(data), data.get("response")];
    for scope in scopes.into_iter().flatten() {
        for key in ["status", "callbackType", "state"] {
            if let Some(s) = scope.get(key).and_then(Value::as_str) {
                let t = s.trim().to_lowercase();
                if !t.is_empty() {
                    return Some(t);
                }
            }
        }
    }
    None
}

fn classify_status(s: &str) -> TaskStatus {
    if FAILURE_STATUSES.contains(&s) {
        return TaskStatus::Failed(s.to_string());
    }
    match s {
        "success" | "complete" => TaskStatus::Success,
        "text_success" => TaskStatus::TextSuccess,
        "first_success" => TaskStatus::FirstSuccess,
        "pending" | "queued" | "submitted" => TaskStatus::Pending,
        other => TaskStatus::Unknown(other.to_string()),
    }
}

/// Track-list location search, hermes' exact order.
fn track_list(data: &Value) -> Option<&Vec<Value>> {
    const PATHS: &[&[&str]] = &[
        &["response", "sunoData"],
        &["response", "data"],
        &["sunoData"],
        &["tracks"],
        &["data"],
    ];
    PATHS.iter().find_map(|path| {
        path.iter()
            .try_fold(data, |v, k| v.get(k))
            .and_then(Value::as_array)
    })
}

fn extract_tracks(data: &Value) -> Vec<Track> {
    let Some(items) = track_list(data) else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|t| t.is_object())
        .map(|t| Track {
            // field-proven preference: the poll path's order, sourceAudioUrl
            // first (the order behind every real successful hermes download)
            audio_url: first_str(
                t,
                &[
                    "sourceAudioUrl",
                    "audioUrl",
                    "audio_url",
                    "sourceStreamAudioUrl",
                    "streamAudioUrl",
                    "stream_audio_url",
                ],
            ),
            id: first_str(t, &["id", "audioId", "trackId"]),
            title: first_str(t, &["title"]).unwrap_or_default(),
            image_url: first_str(t, &["imageUrl", "image_url"]),
            duration: t.get("duration").and_then(Value::as_f64),
            tags: first_str(t, &["tags", "style"]),
        })
        .collect()
}

fn first_str(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| {
        v.get(k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from)
    })
}

fn envelope_code(v: &Value) -> i64 {
    v.get("code").and_then(Value::as_i64).unwrap_or(0)
}

fn envelope_msg(v: &Value) -> String {
    v.get("msg")
        .or_else(|| v.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("unknown error")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_params() -> GenerateParams {
        GenerateParams {
            custom_mode: true,
            instrumental: false,
            model: "V5".into(),
            style: Some("orchestral cinematic mystical".into()),
            title: Some("Trismegistus Fanfare".into()),
            prompt: Some("[Verse]\nGolden chimes".into()),
            ..Default::default()
        }
    }

    #[test]
    fn custom_mode_body_has_the_hermes_shape() {
        let b = base_params().body();
        assert_eq!(b["customMode"], json!(true));
        assert_eq!(b["model"], json!("V5"));
        assert_eq!(b["callBackUrl"], json!(NULL_CALLBACK_URL));
        assert_eq!(b["style"], json!("orchestral cinematic mystical"));
        assert_eq!(b["title"], json!("Trismegistus Fanfare"));
        assert_eq!(b["prompt"], json!("[Verse]\nGolden chimes"));
        assert!(b.get("negativeTags").is_none());
    }

    #[test]
    fn custom_mode_title_is_always_present_even_empty() {
        let mut p = base_params();
        p.title = None;
        assert_eq!(p.body()["title"], json!(""));
    }

    #[test]
    fn instrumental_still_sends_lyrics_but_drops_vocal_gender() {
        let mut p = base_params();
        p.instrumental = true;
        p.vocal_gender = Some("F".into());
        let b = p.body();
        assert_eq!(b["prompt"], json!("[Verse]\nGolden chimes"));
        assert!(b.get("vocalGender").is_none());
        p.instrumental = false;
        assert_eq!(p.body()["vocalGender"], json!("f"));
    }

    #[test]
    fn simple_mode_sends_only_prompt() {
        let p = GenerateParams {
            custom_mode: false,
            instrumental: true,
            model: "V5".into(),
            style: Some("lofi hiphop".into()),
            title: Some("ignored".into()),
            prompt: Some("  a rainy evening beat  ".into()),
            negative_tags: Some("ignored too".into()),
            ..Default::default()
        };
        let b = p.body();
        assert_eq!(b["prompt"], json!("a rainy evening beat"));
        assert!(b.get("style").is_none());
        assert!(b.get("title").is_none());
        assert!(b.get("negativeTags").is_none());
    }

    #[test]
    fn simple_mode_prompt_falls_back_to_style() {
        let p = GenerateParams {
            custom_mode: false,
            model: "V5".into(),
            style: Some("brass fanfare".into()),
            ..Default::default()
        };
        assert_eq!(p.body()["prompt"], json!("brass fanfare"));
    }

    #[test]
    fn sliders_clamp_and_round_to_two_decimals() {
        let mut p = base_params();
        p.weirdness = Some(0.333_33);
        p.style_weight = Some(1.7);
        let b = p.body();
        assert_eq!(b["weirdnessConstraint"], json!(0.33));
        assert_eq!(b["styleWeight"], json!(1.0));
    }

    #[test]
    fn extend_body_is_hermes_shape() {
        let p = ExtendParams {
            task_id: "ae2ad3f9fabcdee05de4deca2e521d9d".into(),
            audio_id: "e3dbbc69-043e-4da9-b5e0-05be9cbb4edd".into(),
            model: "V5".into(),
            continue_at: 0,
            style: Some("brass fanfare".into()),
            ..Default::default()
        };
        let b = p.body();
        assert_eq!(b["audioId"], json!("e3dbbc69-043e-4da9-b5e0-05be9cbb4edd"));
        assert_eq!(b["taskId"], json!("ae2ad3f9fabcdee05de4deca2e521d9d"));
        assert_eq!(b["continueAt"], json!(0));
        assert_eq!(b["callBackUrl"], json!(NULL_CALLBACK_URL));
        assert_eq!(b["style"], json!("brass fanfare"));
        assert!(b.get("prompt").is_none() && b.get("title").is_none());
    }

    #[test]
    fn lyrics_body_is_two_fields_exactly() {
        let b = lyrics_body("a song about rust", None);
        assert_eq!(b["prompt"], json!("a song about rust"));
        assert_eq!(b["callBackUrl"], json!(NULL_CALLBACK_URL));
        assert_eq!(b.as_object().unwrap().len(), 2);
    }

    #[test]
    fn status_tokens_round_trip() {
        assert_eq!(TaskStatus::Pending.as_token(), "pending");
        assert_eq!(TaskStatus::Success.as_token(), "success");
        assert_eq!(
            TaskStatus::Failed("sensitive_word_error".into()).as_token(),
            "sensitive_word_error"
        );
    }

    #[test]
    fn failure_statuses_are_terminal_the_fix_over_hermes() {
        for s in [
            "create_task_failed",
            "generate_audio_failed",
            "callback_exception",
            "sensitive_word_error",
        ] {
            let st = classify_status(s);
            assert_eq!(st, TaskStatus::Failed(s.to_string()));
            assert!(st.is_terminal(), "{s} must terminate the poll loop");
        }
    }

    #[test]
    fn unknown_statuses_keep_polling() {
        let st = classify_status("mystery_stage");
        assert_eq!(st, TaskStatus::Unknown("mystery_stage".into()));
        assert!(!st.is_terminal());
        assert!(!TaskStatus::TextSuccess.is_terminal());
        assert!(!TaskStatus::FirstSuccess.is_terminal());
        assert!(TaskStatus::Success.is_terminal());
    }
}
