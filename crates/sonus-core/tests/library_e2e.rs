//! Library e2e over the shared mini HTTP mock: real downloads onto a real
//! filesystem, hermes filenames, dedupe, honest reporting — and the
//! security assert that the CDN never sees credentials.

mod common;

use std::path::PathBuf;

use common::{audio, serve, Mock};
use sonus_core::{Config, Library, Track};

const TASK: &str = "ae2ad3f9fabcdee05de4deca2e521d9d";

fn fresh_dir(case: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sonus-lib-test-{}-{case}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn library_at(dir: &std::path::Path) -> Library {
    let cfg = Config::resolve(|k| match k {
        "SUNO_DOWNLOAD_DIR" => Some(dir.display().to_string()),
        _ => None,
    });
    Library::new(&cfg).expect("library needs no key")
}

fn track(host: &str, id: &str, title: &str, hex: &str) -> Track {
    Track {
        id: Some(id.into()),
        title: title.into(),
        audio_url: Some(format!("{host}/r/{hex}.mp3")),
        image_url: None,
        duration: Some(168.6),
        tags: Some("orchestral cinematic mystical".into()),
    }
}

#[tokio::test]
async fn downloads_both_variants_with_hermes_names() {
    let dir = fresh_dir("both");
    let (host, rx) = serve(vec![
        audio(b"ID3-fake-audio-one".to_vec()),
        audio(b"ID3-fake-audio-two-longer".to_vec()),
    ]);
    let lib = library_at(&dir);
    let tracks = [
        track(&host, "e3dbbc69", "Trismegistus Fanfare", "e219448d"),
        track(&host, "6042b621", "Trismegistus Fanfare", "d369f931"),
    ];
    let report = lib.download_tracks(TASK, &tracks).await.unwrap();

    assert_eq!(report.files.len(), 2);
    assert!(report.failures.is_empty() && report.undownloadable.is_empty());
    assert_eq!(
        report.files[0].file.file_name().unwrap().to_str().unwrap(),
        "ae2ad3f9fabc__1__Trismegistus_Fanfare.mp3"
    );
    assert_eq!(
        report.files[1].file.file_name().unwrap().to_str().unwrap(),
        "ae2ad3f9fabc__2__Trismegistus_Fanfare.mp3"
    );
    assert_eq!(report.files[0].bytes, 18);
    assert!(!report.files[0].skipped_existing);
    // bytes really landed
    assert_eq!(
        std::fs::read(&report.files[0].file).unwrap(),
        b"ID3-fake-audio-one"
    );
    // no lying .part files left behind
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
        .collect();
    assert!(leftovers.is_empty());
    // SECURITY: the CDN request must carry no credentials at all
    let req = rx.recv().unwrap().to_lowercase();
    assert!(!req.contains("authorization"));
    assert!(!req.contains("bearer"));
}

#[tokio::test]
async fn second_run_dedupes_without_touching_the_network() {
    let dir = fresh_dir("dedupe");
    let (host, _rx) = serve(vec![audio(b"ID3-fake-audio".to_vec())]);
    let lib = library_at(&dir);
    let tracks = [track(&host, "e3dbbc69", "Fanfare", "e219448d")];

    let first = lib.download_tracks(TASK, &tracks).await.unwrap();
    assert!(!first.files[0].skipped_existing);

    // the mock queue is exhausted — a network retry would fail loudly
    let second = lib.download_tracks(TASK, &tracks).await.unwrap();
    assert_eq!(second.files.len(), 1);
    assert!(second.files[0].skipped_existing);
    assert_eq!(second.files[0].bytes, first.files[0].bytes);
}

#[tokio::test]
async fn urlless_tracks_are_reported_not_dropped() {
    let dir = fresh_dir("urlless");
    let lib = library_at(&dir);
    let tracks = [Track {
        id: Some("clip_123".into()),
        title: "Ghost Track".into(),
        audio_url: None,
        image_url: None,
        duration: None,
        tags: None,
    }];
    let report = lib.download_tracks(TASK, &tracks).await.unwrap();
    assert!(report.files.is_empty());
    assert_eq!(report.undownloadable.len(), 1);
    assert!(report.undownloadable[0].contains("Ghost Track"));
}

#[tokio::test]
async fn one_bad_fetch_does_not_kill_the_batch() {
    let dir = fresh_dir("partial");
    let (host, _rx) = serve(vec![
        Mock {
            status: 500,
            content_type: "text/html",
            body: b"cdn sad".to_vec(),
        },
        audio(b"ID3-fake-audio".to_vec()),
    ]);
    let lib = library_at(&dir);
    let tracks = [
        track(&host, "aaa", "Broken One", "dead0001"),
        track(&host, "bbb", "Good One", "dead0002"),
    ];
    let report = lib.download_tracks(TASK, &tracks).await.unwrap();
    assert_eq!(report.files.len(), 1);
    assert_eq!(report.files[0].title, "Good One");
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].contains("Broken One"));
    assert!(report.failures[0].contains("500"));
}

#[tokio::test]
async fn empty_body_is_a_failure_not_a_track() {
    let dir = fresh_dir("empty");
    let (host, _rx) = serve(vec![audio(Vec::new())]);
    let lib = library_at(&dir);
    let tracks = [track(&host, "aaa", "Void", "dead0003")];
    let report = lib.download_tracks(TASK, &tracks).await.unwrap();
    assert!(report.files.is_empty());
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].contains("empty download"));
    // and nothing half-written survives
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
}
