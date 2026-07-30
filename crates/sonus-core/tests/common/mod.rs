//! Dependency-free mini HTTP/1.1 mock shared by the e2e test crates: real
//! sockets, canned responses, one connection per queued response,
//! `Connection: close` keeps it deterministic.
#![allow(dead_code)] // each test crate uses a subset

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

pub struct Mock {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

pub fn json(status: u16, body: &str) -> Mock {
    Mock {
        status,
        content_type: "application/json",
        body: body.as_bytes().to_vec(),
    }
}

pub fn audio(body: Vec<u8>) -> Mock {
    Mock {
        status: 200,
        content_type: "audio/mpeg",
        body,
    }
}

/// Serve the queued responses in order, one per connection. Returns the
/// bare base URL (`http://host:port`) + a receiver yielding each raw
/// request (head + body) for assertions.
pub fn serve(responses: Vec<Mock>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for mock in responses {
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
            let reason = match mock.status {
                200 => "OK",
                404 => "Not Found",
                500 => "Internal Server Error",
                _ => "Error",
            };
            let head = format!(
                "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n",
                mock.status,
                mock.content_type,
                mock.body.len()
            );
            sock.write_all(head.as_bytes()).ok();
            sock.write_all(&mock.body).ok();
        }
    });
    (format!("http://{addr}"), rx)
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
