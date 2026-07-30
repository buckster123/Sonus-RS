//! JSON-RPC dispatch + the tool implementations.
//!
//! Error philosophy (hermes parity, deliberate): domain outcomes — no key,
//! task not ready, upstream refusals — come back as `{"error": …}` payloads
//! in a NORMAL tool result, exactly the dicts hermes returned, so existing
//! soul/procedure guidance keeps working on cutover. JSON-RPC errors are
//! reserved for protocol problems (unknown tool, malformed frame).

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use sonus_core::{
    Config, Credits, ExtendParams, Library, PollOutcome, RecordInfo, SonusError, SunoClient,
    TaskStatus,
};

use crate::{args, tools};

pub struct State {
    pub cfg: Config,
    /// None = no SUNO_API_KEY; money/API tools answer honestly.
    pub client: Option<SunoClient>,
    pub library: Library,
}

impl State {
    pub fn from_env() -> Result<Self, SonusError> {
        let cfg = Config::from_env();
        let client = match SunoClient::new(&cfg) {
            Ok(c) => Some(c),
            Err(SonusError::NotConfigured) => None,
            Err(e) => return Err(e),
        };
        let library = Library::new(&cfg)?;
        Ok(Self {
            cfg,
            client,
            library,
        })
    }
}

const NO_KEY: &str = "SUNO_API_KEY not configured — set it in /etc/sonus/env \
                      (nodes) or the environment (dev), then restart the plugin";

pub fn handle_initialize(req: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": req["id"],
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "sonus-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

pub fn tools_list(req: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": req["id"],
        "result": { "tools": tools::all_tool_schemas() }
    })
}

pub fn method_not_found(req: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": req["id"],
        "error": { "code": -32601, "message": "method not found" }
    })
}

pub fn parse_error() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": null,
        "error": { "code": -32700, "message": "parse error" }
    })
}

pub async fn dispatch_tool(msg: Value, state: Arc<State>) -> Value {
    let id = msg["id"].clone();
    let params = &msg["params"];
    let name = params["name"].as_str().unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    match route(name, &args, &state).await {
        Ok(payload) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": payload.to_string() }]
            }
        }),
        Err(protocol_msg) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32602, "message": protocol_msg }
        }),
    }
}

/// Ok = a tool payload (possibly `{"error": …}` — hermes-shaped domain
/// outcome). Err = protocol-level problem → JSON-RPC error.
async fn route(name: &str, args: &Value, state: &State) -> Result<Value, String> {
    if tools::NOT_YET.contains(&name) {
        return Ok(err_payload(format!(
            "{name} is not implemented yet in Sonus-RS (post-v1). The core \
             compose loop is fully supported: generate_song → \
             check_status_until_done → download_track."
        )));
    }
    match name {
        "check_credits" => Ok(check_credits(state).await),
        "generate_song" => Ok(generate_song(args, state).await),
        "check_status" => Ok(check_status(args, state).await),
        "check_status_until_done" => Ok(check_status_until_done(args, state).await),
        "download_track" => Ok(download_track(args, state).await),
        "extend_track" => Ok(extend_track(args, state).await),
        "generate_lyrics" => Ok(generate_lyrics(args, state).await),
        other => Err(format!("unknown tool: {other}")),
    }
}

fn err_payload(msg: impl Into<String>) -> Value {
    json!({ "error": msg.into() })
}

fn client(state: &State) -> Result<&SunoClient, Value> {
    state.client.as_ref().ok_or_else(|| err_payload(NO_KEY))
}

fn snapshot(info: &RecordInfo) -> Value {
    json!({
        "task_id": info.task_id,
        "status": info.status.as_token(),
        "is_complete": info.status == TaskStatus::Success,
        "is_failed": matches!(info.status, TaskStatus::Failed(_)),
        "error_message": info.error_message,
        "tracks": info.tracks.iter().map(|t| json!({
            "id": t.id,
            "title": t.title,
            "duration": t.duration,
            "audio_url": t.audio_url,
        })).collect::<Vec<_>>(),
    })
}

async fn check_credits(state: &State) -> Value {
    let client = match client(state) {
        Ok(c) => c,
        Err(e) => return e,
    };
    match client.credits().await {
        Ok(Credits::Known { remaining, total }) => json!({
            "credits_remaining": remaining,
            "credits_total": total.map(Value::from).unwrap_or_else(|| "unknown".into()),
        }),
        Ok(Credits::Unknown) => json!({
            "credits_remaining": "unknown",
            "credits_total": "unknown",
            "note": "the credits endpoint is not exposed by this sunoapi.org instance",
        }),
        Err(e) => err_payload(e.to_string()),
    }
}

async fn generate_song(args: &Value, state: &State) -> Value {
    let client = match client(state) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let (params, warnings) = match args::generate_params(args) {
        Ok(x) => x,
        Err(e) => return err_payload(e),
    };
    match client.generate(&params).await {
        Ok(task_id) => json!({
            "task_id": task_id,
            "model": params.model,
            "instrumental": params.instrumental,
            "warnings": warnings,
            "next": "check_status_until_done with this task_id, then download_track",
        }),
        Err(e) => err_payload(e.to_string()),
    }
}

async fn check_status(args: &Value, state: &State) -> Value {
    let client = match client(state) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let Some(task_id) = args::str_arg(args, "task_id") else {
        return err_payload("task_id is required");
    };
    match client.record_info(&task_id).await {
        Ok(info) => snapshot(&info),
        Err(e) => err_payload(e.to_string()),
    }
}

async fn check_status_until_done(args: &Value, state: &State) -> Value {
    let client = match client(state) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let Some(task_id) = args::str_arg(args, "task_id") else {
        return err_payload("task_id is required");
    };
    let timeout = args
        .get("timeout_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(300)
        .clamp(5, 3600);
    match client
        .poll_until_done(&task_id, Duration::from_secs(timeout))
        .await
    {
        Ok(PollOutcome::Done(info)) => snapshot(&info),
        Ok(PollOutcome::TimedOut { last, waited }) => json!({
            "task_id": task_id,
            "status": "timeout",
            "is_complete": false,
            "is_failed": false,
            "waited_seconds": waited.as_secs(),
            "resumable": true,
            "last_status": last.map(|i| i.status.as_token().to_string()),
            "note": "the paid task is still live upstream — call \
                     check_status_until_done again with the same task_id",
        }),
        Err(e) => err_payload(e.to_string()),
    }
}

async fn download_track(args: &Value, state: &State) -> Value {
    let client = match client(state) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let Some(task_id) = args::str_arg(args, "task_id") else {
        return err_payload("task_id is required");
    };
    let info = match client.record_info(&task_id).await {
        Ok(i) => i,
        Err(e) => return err_payload(e.to_string()),
    };
    if let TaskStatus::Failed(reason) = &info.status {
        return err_payload(format!(
            "generation failed ({reason}){} — nothing to download",
            info.error_message
                .as_deref()
                .map(|m| format!(": {m}"))
                .unwrap_or_default()
        ));
    }
    if info.tracks.is_empty() {
        return err_payload(format!(
            "no tracks ready yet (status: {}) — use check_status_until_done first",
            info.status.as_token()
        ));
    }
    let selected: Vec<_> = match args.get("track_index").and_then(Value::as_u64) {
        None => info.tracks.clone(),
        Some(0) => return err_payload("track_index is 1-based"),
        Some(i) => match info.tracks.get(i as usize - 1) {
            Some(t) => vec![t.clone()],
            None => {
                return err_payload(format!(
                    "track_index {i} out of range — the task has {} track(s)",
                    info.tracks.len()
                ))
            }
        },
    };
    // optional per-call dir override (hermes arg parity)
    let override_lib;
    let lib = match args::str_arg(args, "download_dir") {
        Some(dir) => {
            let mut cfg = state.cfg.clone();
            cfg.download_dir = dir.into();
            override_lib = match Library::new(&cfg) {
                Ok(l) => l,
                Err(e) => return err_payload(e.to_string()),
            };
            &override_lib
        }
        None => &state.library,
    };
    match lib.download_tracks(&task_id, &selected).await {
        Ok(report) => json!({
            "dir": lib.dir().display().to_string(),
            "files": report.files.iter().map(|f| json!({
                "path": f.file.display().to_string(),
                "bytes": f.bytes,
                "title": f.title,
                "already_existed": f.skipped_existing,
            })).collect::<Vec<_>>(),
            "failures": report.failures,
            "undownloadable": report.undownloadable,
        }),
        Err(e) => err_payload(e.to_string()),
    }
}

async fn extend_track(args: &Value, state: &State) -> Value {
    let client = match client(state) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let (Some(task_id), Some(audio_id)) = (
        args::str_arg(args, "task_id"),
        args::str_arg(args, "audio_id"),
    ) else {
        return err_payload("task_id and audio_id are both required");
    };
    let params = ExtendParams {
        task_id,
        audio_id,
        model: args::str_arg(args, "model").unwrap_or_else(|| "V5".to_string()),
        continue_at: args.get("continue_at").and_then(Value::as_i64).unwrap_or(0),
        prompt: args::str_arg(args, "prompt"),
        style: args::str_arg(args, "style"),
        title: args::str_arg(args, "title"),
        callback_url: args::str_arg(args, "callback_url"),
    };
    match client.extend(&params).await {
        Ok(new_task) => json!({
            "task_id": new_task,
            "note": "the extension is a new task — poll it like a generation",
        }),
        Err(e) => err_payload(e.to_string()),
    }
}

async fn generate_lyrics(args: &Value, state: &State) -> Value {
    let client = match client(state) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let Some(prompt) = args::str_arg(args, "prompt") else {
        return err_payload("prompt is required");
    };
    let callback = args::str_arg(args, "callback_url");
    match client.lyrics(&prompt, callback.as_deref()).await {
        Ok(task_id) => json!({
            "task_id": task_id,
            "note": "v1 honesty: upstream lyrics tasks use a separate \
                     record-info this server doesn't poll yet (post-v1) — \
                     treat as fire-and-forget for now",
        }),
        Err(e) => err_payload(e.to_string()),
    }
}
