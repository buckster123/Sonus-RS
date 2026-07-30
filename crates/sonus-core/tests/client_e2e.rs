//! Client tests over the shared mini HTTP mock (tests/common) — real
//! reqwest, canned envelopes.

mod common;

use std::time::Duration;

use common::{json, serve};
use sonus_core::types::TaskStatus;
use sonus_core::{Config, GenerateParams, PollOutcome, SonusError, SunoClient};

fn client_for(host: &str) -> SunoClient {
    let base = format!("{host}/api/v1");
    let cfg = Config::resolve(|k| match k {
        "SUNO_API_BASE" => Some(base.clone()),
        "SUNO_API_KEY" => Some("test-key-123".to_string()),
        _ => None,
    });
    SunoClient::new(&cfg)
        .expect("key present")
        .with_initial_poll_interval(Duration::from_millis(10))
}

#[tokio::test]
async fn generate_then_poll_to_success() {
    let (host, rx) = serve(vec![
        json(200, include_str!("fixtures/generate_ok.json")),
        json(200, include_str!("fixtures/record_info_pending.json")),
        json(200, include_str!("fixtures/record_info_success.json")),
    ]);
    let client = client_for(&host);

    let params = GenerateParams {
        custom_mode: true,
        instrumental: true,
        model: "V5".into(),
        style: Some("orchestral cinematic mystical".into()),
        title: Some("Trismegistus Fanfare".into()),
        ..Default::default()
    };
    let task = client.generate(&params).await.unwrap();
    assert_eq!(task, "ae2ad3f9fabcdee05de4deca2e521d9d");

    // the wire request: bearer auth + camelCase body, key never in the URL
    let req = rx.recv().unwrap();
    let lower = req.to_lowercase();
    assert!(lower.contains("authorization: bearer test-key-123"));
    assert!(req.contains("\"customMode\":true"));
    assert!(req.contains("\"callBackUrl\":\"https://localhost/callback\""));
    assert!(!req.lines().next().unwrap_or("").contains("test-key-123"));

    let outcome = client
        .poll_until_done(&task, Duration::from_secs(5))
        .await
        .unwrap();
    match outcome {
        PollOutcome::Done(info) => {
            assert_eq!(info.status, TaskStatus::Success);
            assert_eq!(info.tracks.len(), 2);
        }
        other => panic!("expected Done, got {other:?}"),
    }
    // the poll request carried the task id as a query param
    let poll_req = rx.recv().unwrap();
    assert!(poll_req.contains("record-info?taskId=ae2ad3f9fabcdee05de4deca2e521d9d"));
}

#[tokio::test]
async fn poll_timeout_is_resumable_not_an_error() {
    let (host, _rx) = serve(vec![
        json(200, include_str!("fixtures/record_info_pending.json")),
        json(200, include_str!("fixtures/record_info_pending.json")),
    ]);
    let client = client_for(&host);
    let outcome = client
        .poll_until_done(
            "ae2ad3f9fabcdee05de4deca2e521d9d",
            Duration::from_millis(40),
        )
        .await
        .unwrap();
    match outcome {
        PollOutcome::TimedOut { last, waited } => {
            let last = last.expect("last snapshot rides along for the caller");
            assert_eq!(last.status, TaskStatus::Pending);
            assert!(waited <= Duration::from_millis(200));
        }
        other => panic!("expected TimedOut, got {other:?}"),
    }
}

#[tokio::test]
async fn fatal_poll_error_surfaces_immediately() {
    let (host, _rx) = serve(vec![json(200, r#"{"code":401,"msg":"unauthorized"}"#)]);
    let client = client_for(&host);
    let err = client
        .poll_until_done("deadbeef", Duration::from_secs(5))
        .await
        .unwrap_err();
    assert!(matches!(err, SonusError::Api { code: 401, .. }));
}

#[tokio::test]
async fn failed_generation_terminates_the_poll() {
    let (host, _rx) = serve(vec![json(
        200,
        include_str!("fixtures/record_info_failed_sensitive.json"),
    )]);
    let client = client_for(&host);
    let outcome = client
        .poll_until_done("ae2ad3f9fabcdee05de4deca2e521d9d", Duration::from_secs(5))
        .await
        .unwrap();
    match outcome {
        PollOutcome::Done(info) => {
            assert_eq!(
                info.status,
                TaskStatus::Failed("sensitive_word_error".into())
            );
            assert_eq!(
                info.error_message.as_deref(),
                Some("prompt contains blocked words")
            );
        }
        other => panic!("expected Done(Failed), got {other:?}"),
    }
}

#[tokio::test]
async fn credits_http_404_is_unknown() {
    let (host, _rx) = serve(vec![common::Mock {
        status: 404,
        content_type: "text/html",
        body: b"<html>nope</html>".to_vec(),
    }]);
    let client = client_for(&host);
    assert_eq!(
        client.credits().await.unwrap(),
        sonus_core::Credits::Unknown
    );
}

#[tokio::test]
async fn credits_number_shape_over_the_wire() {
    let (host, _rx) = serve(vec![json(
        200,
        include_str!("fixtures/credits_number.json"),
    )]);
    let client = client_for(&host);
    assert_eq!(
        client.credits().await.unwrap(),
        sonus_core::Credits::Known {
            remaining: 437.2,
            total: None
        }
    );
}
