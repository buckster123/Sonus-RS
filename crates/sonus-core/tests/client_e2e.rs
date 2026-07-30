//! Client tests against a dependency-free mini HTTP/1.1 mock: real sockets,
//! real reqwest, canned bodies. One connection per queued response,
//! `Connection: close` keeps it deterministic.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use sonus_core::types::TaskStatus;
use sonus_core::{Config, GenerateParams, PollOutcome, SonusError, SunoClient};

/// Serve the queued (status, body) responses in order, one per connection.
/// Returns the api_base to point the client at + a receiver yielding each
/// raw request (head + body) for assertions.
fn serve(responses: Vec<(u16, String)>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut sock, _)) = listener.accept() else {
                return;
            };
            sock.set_read_timeout(Some(Duration::from_secs(2))).ok();
            let mut buf = Vec::new();
            let mut tmp = [0u8; 4096];
            loop {
                match sock.read(&mut tmp) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(head_end) = find(&buf, b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&buf[..head_end]);
                            let cl = content_length(&head);
                            if buf.len() >= head_end + 4 + cl {
                                break;
                            }
                        }
                    }
                }
            }
            tx.send(String::from_utf8_lossy(&buf).into_owned()).ok();
            let reason = match status {
                200 => "OK",
                404 => "Not Found",
                _ => "Error",
            };
            let resp = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(resp.as_bytes()).ok();
        }
    });
    (format!("http://{addr}/api/v1"), rx)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn content_length(head: &str) -> usize {
    head.lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse().ok())?
        })
        .unwrap_or(0)
}

fn client_for(base: &str) -> SunoClient {
    let cfg = Config::resolve(|k| match k {
        "SUNO_API_BASE" => Some(base.to_string()),
        "SUNO_API_KEY" => Some("test-key-123".to_string()),
        _ => None,
    });
    SunoClient::new(&cfg)
        .expect("key present")
        .with_initial_poll_interval(Duration::from_millis(10))
}

#[tokio::test]
async fn generate_then_poll_to_success() {
    let (base, rx) = serve(vec![
        (200, include_str!("fixtures/generate_ok.json").into()),
        (
            200,
            include_str!("fixtures/record_info_pending.json").into(),
        ),
        (
            200,
            include_str!("fixtures/record_info_success.json").into(),
        ),
    ]);
    let client = client_for(&base);

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
    let (base, _rx) = serve(vec![
        (
            200,
            include_str!("fixtures/record_info_pending.json").into(),
        ),
        (
            200,
            include_str!("fixtures/record_info_pending.json").into(),
        ),
    ]);
    let client = client_for(&base);
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
    let (base, _rx) = serve(vec![(200, r#"{"code":401,"msg":"unauthorized"}"#.into())]);
    let client = client_for(&base);
    let err = client
        .poll_until_done("deadbeef", Duration::from_secs(5))
        .await
        .unwrap_err();
    assert!(matches!(err, SonusError::Api { code: 401, .. }));
}

#[tokio::test]
async fn failed_generation_terminates_the_poll() {
    let (base, _rx) = serve(vec![(
        200,
        include_str!("fixtures/record_info_failed_sensitive.json").into(),
    )]);
    let client = client_for(&base);
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
    let (base, _rx) = serve(vec![(404, "<html>nope</html>".into())]);
    let client = client_for(&base);
    assert_eq!(
        client.credits().await.unwrap(),
        sonus_core::Credits::Unknown
    );
}

#[tokio::test]
async fn credits_number_shape_over_the_wire() {
    let (base, _rx) = serve(vec![(
        200,
        include_str!("fixtures/credits_number.json").into(),
    )]);
    let client = client_for(&base);
    assert_eq!(
        client.credits().await.unwrap(),
        sonus_core::Credits::Known {
            remaining: 437.2,
            total: None
        }
    );
}
