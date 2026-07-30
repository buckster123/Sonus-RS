//! Pure-parser tests over the fixture set (tests/fixtures/README.md for
//! provenance — shapes from hermes' parsing code, values from a real run).

use serde_json::Value;
use sonus_core::types::{parse_credits, parse_record_info, parse_task_id, Credits, TaskStatus};
use sonus_core::SonusError;

fn fx(name: &str) -> Value {
    let raw = match name {
        "generate_ok" => include_str!("fixtures/generate_ok.json"),
        "generate_insufficient_credits" => {
            include_str!("fixtures/generate_insufficient_credits.json")
        }
        "record_info_pending" => include_str!("fixtures/record_info_pending.json"),
        "record_info_success" => include_str!("fixtures/record_info_success.json"),
        "record_info_failed_sensitive" => {
            include_str!("fixtures/record_info_failed_sensitive.json")
        }
        "record_info_failed_hermes_mock" => {
            include_str!("fixtures/record_info_failed_hermes_mock.json")
        }
        "record_info_tracks_no_status" => {
            include_str!("fixtures/record_info_tracks_no_status.json")
        }
        "credits_number" => include_str!("fixtures/credits_number.json"),
        "credits_object" => include_str!("fixtures/credits_object.json"),
        other => panic!("unknown fixture {other}"),
    };
    serde_json::from_str(raw).expect("fixture is valid JSON")
}

#[test]
fn generate_ok_yields_the_task_id() {
    assert_eq!(
        parse_task_id(&fx("generate_ok")).unwrap(),
        "ae2ad3f9fabcdee05de4deca2e521d9d"
    );
}

#[test]
fn generate_429_is_an_honest_credits_error() {
    let err = parse_task_id(&fx("generate_insufficient_credits")).unwrap_err();
    match &err {
        SonusError::Api { code: 429, msg } => assert!(msg.contains("insufficient credits")),
        other => panic!("expected Api 429, got {other:?}"),
    }
    assert!(err.is_fatal());
}

#[test]
fn snake_case_task_id_fallback() {
    let v: Value = serde_json::from_str(r#"{"code":200,"data":{"task_id":"abc123"}}"#).unwrap();
    assert_eq!(parse_task_id(&v).unwrap(), "abc123");
}

#[test]
fn pending_snapshot() {
    let info = parse_record_info(&fx("record_info_pending")).unwrap();
    assert_eq!(info.status, TaskStatus::Pending);
    assert!(!info.status.is_terminal());
    assert!(info.tracks.is_empty());
    assert_eq!(
        info.task_id.as_deref(),
        Some("ae2ad3f9fabcdee05de4deca2e521d9d")
    );
}

#[test]
fn success_snapshot_carries_both_variants() {
    let info = parse_record_info(&fx("record_info_success")).unwrap();
    assert_eq!(info.status, TaskStatus::Success);
    assert!(info.status.is_terminal());
    assert_eq!(info.tracks.len(), 2, "suno returns two variants");
    let t = &info.tracks[0];
    assert_eq!(
        t.id.as_deref(),
        Some("e3dbbc69-043e-4da9-b5e0-05be9cbb4edd")
    );
    assert_eq!(t.title, "Trismegistus Fanfare");
    // sourceAudioUrl wins over streamAudioUrl (field-proven order)
    assert_eq!(
        t.audio_url.as_deref(),
        Some("https://tempfile.aiquickdraw.com/r/e219448d8f2d41d491a321766c7e38bd.mp3")
    );
    assert_eq!(t.duration, Some(168.6));
    assert_eq!(t.tags.as_deref(), Some("orchestral cinematic mystical"));
    assert_eq!(info.tracks[1].duration, Some(172.72));
    assert!(info.error_message.is_none());
}

#[test]
fn documented_failure_status_is_terminal_with_reason() {
    let info = parse_record_info(&fx("record_info_failed_sensitive")).unwrap();
    assert_eq!(
        info.status,
        TaskStatus::Failed("sensitive_word_error".into())
    );
    assert!(info.status.is_terminal());
    assert_eq!(
        info.error_message.as_deref(),
        Some("prompt contains blocked words")
    );
}

#[test]
fn hermes_mock_failure_shape_still_parses() {
    let info = parse_record_info(&fx("record_info_failed_hermes_mock")).unwrap();
    assert_eq!(info.status, TaskStatus::Failed("failed".into()));
    assert_eq!(
        info.error_message.as_deref(),
        Some("Content policy violation"),
        "errorMessage inside data.response must be found"
    );
}

#[test]
fn tracks_without_status_mean_complete() {
    let info = parse_record_info(&fx("record_info_tracks_no_status")).unwrap();
    assert_eq!(info.status, TaskStatus::Success);
    assert_eq!(info.tracks.len(), 1);
    let t = &info.tracks[0];
    assert_eq!(
        t.audio_url.as_deref(),
        Some("https://example.com/audio.mp3")
    );
    assert_eq!(t.id.as_deref(), Some("clip_123"));
    assert_eq!(t.duration, Some(120.0));
}

#[test]
fn envelope_error_surfaces_as_api_error() {
    let v: Value = serde_json::from_str(r#"{"code":401,"msg":"bad key"}"#).unwrap();
    let err = parse_record_info(&v).unwrap_err();
    assert!(matches!(err, SonusError::Api { code: 401, .. }));
    assert!(err.is_fatal());
}

#[test]
fn credits_number_shape() {
    assert_eq!(
        parse_credits(200, &fx("credits_number")).unwrap(),
        Credits::Known {
            remaining: 437.2,
            total: None
        }
    );
}

#[test]
fn credits_object_shape() {
    assert_eq!(
        parse_credits(200, &fx("credits_object")).unwrap(),
        Credits::Known {
            remaining: 120.0,
            total: Some(500.0)
        }
    );
}

#[test]
fn credits_404_is_honest_unknown_both_ways() {
    // HTTP-level 404
    assert_eq!(parse_credits(404, &Value::Null).unwrap(), Credits::Unknown);
    // envelope-level 404
    let v: Value = serde_json::from_str(r#"{"code":404,"msg":"not found"}"#).unwrap();
    assert_eq!(parse_credits(200, &v).unwrap(), Credits::Unknown);
}
