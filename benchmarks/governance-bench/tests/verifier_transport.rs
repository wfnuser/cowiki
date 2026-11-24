#![cfg(feature = "remote-verifier")]
use cowiki_governance_bench::{evidence_packet, load_dataset};
use cowiki_knowledge::governance::remote::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::Path,
};

#[test]
fn record_and_replay_one_real_http_exchange() {
    let data = load_dataset(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pilot"),
        "dev",
    )
    .unwrap();
    let case = &data.cases[0];
    let p = evidence_packet(&data.families[&case.family], case).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(&socket);
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some(n) = line.to_lowercase().strip_prefix("content-length: ") {
                length = n.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(request["max_tokens"], 768);
        let response=serde_json::json!({"model":"fixture","choices":[{"message":{"content":"{\"judgments\":[],\"findings\":[],\"edits\":[]}"}}],"usage":{"prompt_tokens":123,"completion_tokens":17}}).to_string();
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
    });
    let cache = tempfile::tempdir().unwrap();
    let mut provider = RemoteVerifier {
        endpoint,
        model: "fixture".into(),
        api_key: None,
        cache_dir: cache.path().into(),
        mode: CacheMode::Record,
    };
    let first = provider.request(&p, Task::SinglePass).unwrap();
    server.join().unwrap();
    assert_eq!(
        (
            first.usage.http_calls,
            first.usage.prompt_tokens,
            first.usage.completion_tokens
        ),
        (1, 123, 17)
    );
    provider.mode = CacheMode::Replay;
    let replay = provider.request(&p, Task::SinglePass).unwrap();
    assert_eq!(replay.key, first.key);
    assert_eq!(
        (
            replay.usage.calls,
            replay.usage.http_calls,
            replay.usage.cache_hits
        ),
        (1, 0, 1)
    );
    provider.model = "other".into();
    assert!(provider.request(&p, Task::SinglePass).is_err());
}

#[test]
fn failed_attempt_is_charged_once_and_replayed_offline() {
    let data = load_dataset(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pilot"),
        "dev",
    )
    .unwrap();
    let case = &data.cases[0];
    let packet = evidence_packet(&data.families[&case.family], case).unwrap();
    let cache = tempfile::tempdir().unwrap();
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut provider = RemoteVerifier {
        endpoint: format!("http://{}", socket.local_addr().unwrap()),
        model: "mock".into(),
        api_key: None,
        cache_dir: cache.path().into(),
        mode: CacheMode::Record,
    };
    let worker = std::thread::spawn(move || {
        let (mut s, _) = socket.accept().unwrap();
        s.write_all(
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .unwrap()
    });
    let mut attempt = Attempt::default();
    assert!(provider
        .request_accounted(&packet, Task::Verify, &mut attempt)
        .is_err());
    worker.join().unwrap();
    assert_eq!((attempt.usage.calls, attempt.usage.http_calls), (1, 1));
    assert!(attempt.usage.estimated);
    provider.mode = CacheMode::Replay;
    assert!(provider
        .request_accounted(&packet, Task::Verify, &mut attempt)
        .is_err());
    assert_eq!(
        (
            attempt.usage.calls,
            attempt.usage.http_calls,
            attempt.usage.cache_hits
        ),
        (1, 0, 1)
    );
}
