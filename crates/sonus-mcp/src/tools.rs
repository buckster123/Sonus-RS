//! The tool surface — hermes-sonus names, EXACTLY (the drop-in contract).
//! Descriptions double as the agent's operating reference (the A7 lesson):
//! the compose loop is spelled out where the model reads it.

use serde_json::{json, Value};

/// The 9 hermes tools v1 answers with an honest not-yet (post-v1 pulls).
pub const NOT_YET: &[&str] = &[
    "generate_album",
    "separate_stems",
    "replace_section",
    "boost_style",
    "convert_to_wav",
    "create_music_video",
    "generate_sounds",
    "clone_voice_validate",
    "clone_voice_create",
];

fn tool(name: &str, description: &str, props: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": required,
        }
    })
}

pub fn all_tool_schemas() -> Vec<Value> {
    let mut tools = vec![
        tool(
            "generate_song",
            "Generate a Suno song (SPENDS CREDITS — check_credits first is free). \
             The compose loop: generate_song → check_status_until_done → download_track. \
             Suno returns 2 variants per generation, usually in 30-180s.",
            json!({
                "styles": {"type": "string", "description": "Comma-separated style descriptors, e.g. \"dream pop, breathy female vocals, 80BPM, warm reverb\""},
                "lyrics": {"type": "string", "description": "Lyrics with optional [Verse]/[Chorus] tags. Leave empty + instrumental for pure music."},
                "exclude_styles": {"type": "string", "description": "Styles to avoid, comma-separated"},
                "title": {"type": "string", "description": "Optional; blank lets Suno auto-title (often better)"},
                "weirdness_pct": {"type": "number", "description": "0-100 experimental dial (default 50)"},
                "style_pct": {"type": "number", "description": "0-100 style-adherence dial (default 50)"},
                "unhinged_seed": {"type": "string", "description": "Optional chaos seed, rides the lyrics field"},
                "model": {"type": "string", "enum": ["V4", "V4_5", "V4_5PLUS", "V4_5ALL", "V5", "V5_5"], "description": "Default V5"},
                "instrumental": {"type": "boolean", "description": "Omit to auto-detect from lyrics"},
                "vocal_gender": {"type": "string", "enum": ["m", "f"], "description": "Only used on vocal songs"},
                "callback_url": {"type": "string", "description": "Optional; this server is poll-based, leave unset"}
            }),
            &["styles"],
        ),
        tool(
            "check_status",
            "One status poll for a generation task. For waiting, prefer \
             check_status_until_done.",
            json!({
                "task_id": {"type": "string", "description": "The task_id from generate_song/extend_track"}
            }),
            &["task_id"],
        ),
        tool(
            "check_status_until_done",
            "Poll a task until it completes, fails, or the timeout passes \
             (default 300s). A timeout is NOT failure: the paid task stays \
             live upstream — call again with the same task_id to resume.",
            json!({
                "task_id": {"type": "string"},
                "timeout_seconds": {"type": "integer", "description": "Max wait, default 300, clamp 5-3600"}
            }),
            &["task_id"],
        ),
        tool(
            "download_track",
            "Download a completed task's audio into the sonus library \
             (SUNO_DOWNLOAD_DIR — the player and SCORE picker see it \
             immediately). Re-downloads dedupe by filename.",
            json!({
                "task_id": {"type": "string"},
                "download_dir": {"type": "string", "description": "Optional override; default SUNO_DOWNLOAD_DIR"},
                "track_index": {"type": "integer", "description": "1-based; omit to download all variants"}
            }),
            &["task_id"],
        ),
        tool(
            "extend_track",
            "Extend an existing variant with more music (SPENDS CREDITS). \
             Returns a NEW task_id — poll it like a generation.",
            json!({
                "task_id": {"type": "string", "description": "The original generation task"},
                "audio_id": {"type": "string", "description": "The variant to extend (track id from check_status)"},
                "prompt": {"type": "string", "description": "Optional lyrics for the extension"},
                "style": {"type": "string", "description": "Optional style override"},
                "title": {"type": "string"},
                "continue_at": {"type": "integer", "description": "Seconds into the original; 0 = from the end"},
                "model": {"type": "string", "description": "Default V5"},
                "callback_url": {"type": "string", "description": "Optional; poll-based server, leave unset"}
            }),
            &["task_id", "audio_id"],
        ),
        tool(
            "generate_lyrics",
            "Generate lyrics only (SPENDS CREDITS). v1 honesty: upstream \
             uses a separate lyrics record-info this server doesn't poll yet \
             — treat as fire-and-forget until post-v1.",
            json!({
                "prompt": {"type": "string", "description": "Short description of the lyrical content (~200 chars best)"},
                "callback_url": {"type": "string", "description": "Optional; leave unset"}
            }),
            &["prompt"],
        ),
        tool(
            "check_credits",
            "Remaining sunoapi.org credits. FREE — always check before \
             spending on generate_song/extend_track.",
            json!({}),
            &[],
        ),
    ];
    for name in NOT_YET {
        tools.push(tool(
            name,
            "Not implemented yet in Sonus-RS (post-v1). The core compose \
             loop IS fully supported: generate_song → check_status_until_done \
             → download_track.",
            json!({}),
            &[],
        ));
    }
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_full_hermes_surface_is_registered() {
        let tools = all_tool_schemas();
        assert_eq!(tools.len(), 16, "7 core + 9 honest not-yets");
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        for core in [
            "generate_song",
            "check_status",
            "check_status_until_done",
            "download_track",
            "extend_track",
            "generate_lyrics",
            "check_credits",
        ] {
            assert!(names.contains(&core), "{core} missing");
        }
        for stub in NOT_YET {
            assert!(names.contains(stub), "{stub} missing");
        }
    }

    #[test]
    fn schemas_declare_their_required_args() {
        let tools = all_tool_schemas();
        let find = |n: &str| tools.iter().find(|t| t["name"] == n).unwrap();
        assert_eq!(
            find("generate_song")["inputSchema"]["required"],
            json!(["styles"])
        );
        assert_eq!(
            find("extend_track")["inputSchema"]["required"],
            json!(["task_id", "audio_id"])
        );
        assert_eq!(find("check_credits")["inputSchema"]["required"], json!([]));
    }
}
