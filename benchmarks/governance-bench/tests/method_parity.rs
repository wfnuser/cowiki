#![cfg(feature = "remote-verifier")]
use cowiki_governance_bench::{
    evaluation::{Mode, Runner},
    evidence_packet, load_dataset,
};
use cowiki_knowledge::governance::{
    protocol::{self, Usage},
    remote::*,
};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::Path,
};

fn read_request(socket: &std::net::TcpStream) -> Value {
    let mut reader = BufReader::new(socket);
    let mut len = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        if let Some(l) = line.to_lowercase().strip_prefix("content-length: ") {
            len = l.trim().parse().unwrap()
        }
    }
    let mut data = vec![0; len];
    reader.read_exact(&mut data).unwrap();
    serde_json::from_slice(&data).unwrap()
}
#[test]
fn same_packet_equal_caps_cited_disagreement_and_single_call_replay() {
    let d = load_dataset(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pilot"),
        "dev",
    )
    .unwrap();
    let case = &d.cases[0];
    let packet = protocol::pack(evidence_packet(&d.families[&case.family], case).unwrap());
    let expected = model_input(&packet);
    assert!(expected.get("gold").is_none());
    assert!(expected.get("issues").is_none());
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", server.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        for verify in [false, true] {
            let (mut socket, _) = server.accept().unwrap();
            let req = read_request(&socket);
            assert_eq!(req["max_tokens"], 768);
            assert_eq!(req["temperature"], 0);
            let input: Value =
                serde_json::from_str(req["messages"][1]["content"].as_str().unwrap()).unwrap();
            assert_eq!(input, expected);
            let mut pairs = vec![];
            if verify {
                for c in 0..input["changed"].as_array().unwrap().len() {
                    for e in 0..input["evidence"].as_array().unwrap().len() {
                        pairs.push(json!({"candidate":c,"evidence":e,"relation":"neutral"}));
                    }
                }
            }
            let answer = json!({"judgments":pairs,"findings":[],"edits":[]});
            let res =
                json!({"model":"fixture","choices":[{"message":{"content":answer.to_string()}}],
                "usage":{"prompt_tokens":201,"completion_tokens":51}})
                .to_string();
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                res.len(),
                res
            )
            .unwrap();
        }
    });
    let cache = tempfile::tempdir().unwrap();
    let mut runner = Runner {
        provider: Some(RemoteVerifier {
            endpoint,
            model: "fixture".into(),
            api_key: None,
            cache_dir: cache.path().into(),
            mode: CacheMode::Record,
            limits: RequestLimits::default(),
        }),
    };
    for mode in [
        Mode::None,
        Mode::Rules,
        Mode::SinglePass,
        Mode::Remote,
        Mode::Both,
    ] {
        let result = runner.run(mode, &packet);
        assert_eq!(result.error, None);
        let Usage {
            calls,
            http_calls,
            prompt_tokens,
            ..
        } = result.usage;
        assert_eq!(calls, u64::from(mode.uses_model()));
        assert_eq!(
            http_calls,
            u64::from(matches!(mode, Mode::SinglePass | Mode::Remote))
        );
        if mode.uses_model() {
            assert_eq!(prompt_tokens, 201)
        }
        if mode == Mode::Both {
            let result = result.maintenance.unwrap();
            assert!(!result.initial.is_empty());
            assert!(result.proposals.is_empty());
            assert!(result
                .unresolved
                .iter()
                .all(|i| i.message.contains("disagree")));
        }
    }
    worker.join().unwrap();
    runner.provider.as_mut().unwrap().mode = CacheMode::Replay;
    for mode in [Mode::Remote, Mode::Both, Mode::SinglePass] {
        let out = runner.run(mode, &packet);
        assert_eq!(out.error, None);
        assert_eq!(
            (out.usage.calls, out.usage.http_calls, out.usage.cache_hits),
            (1, 0, 1)
        );
    }
}
