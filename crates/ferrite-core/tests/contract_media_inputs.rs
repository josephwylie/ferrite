#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use base64::Engine;
use std::fs;
use support::*;

#[test]
fn attachments_use_native_pdf_and_audio_only_where_supported() {
    let dir = std::env::temp_dir().join(format!("ferrite-media-input-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let pdf = dir.join("reference.PDF");
    let audio = dir.join("recording.MP3");
    let video = dir.join("clip.mp4");
    fs::write(&pdf, b"%PDF-1.7\nfixture").unwrap();
    fs::write(&audio, b"ID3fixture").unwrap();
    fs::write(&video, b"fixture").unwrap();
    let prompt =
        ferrite_core::prompt_files::compose("Explain these", &[pdf.clone(), audio.clone(), video]);
    for provider in ["claude", "codex"] {
        let mut replay = Replay::new(provider, vec![]);
        replay.drain();
        replay.session.send(&prompt).unwrap();
        if provider == "claude" {
            let frame = replay.wait_host(|v| v["type"] == "user");
            let blocks = frame["message"]["content"].as_array().unwrap();
            let document = blocks
                .iter()
                .find(|b| b["type"] == "document")
                .expect("PDF is native document input");
            assert_eq!(document["source"]["media_type"], "application/pdf");
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(document["source"]["data"].as_str().unwrap())
                    .unwrap(),
                b"%PDF-1.7\nfixture"
            );
            assert!(blocks.iter().all(|b| b["type"] != "audio"));
            assert!(blocks[0]["text"]
                .as_str()
                .unwrap()
                .contains("recording.MP3"));
        } else {
            let frame = replay.wait_host(|v| v["method"] == "turn/start");
            let blocks = frame["params"]["input"].as_array().unwrap();
            assert!(blocks
                .iter()
                .any(|b| b["type"] == "localAudio" && b["path"] == audio.to_str().unwrap()));
            assert!(blocks
                .iter()
                .any(|b| b["type"] == "mention" && b["path"] == pdf.to_str().unwrap()));
            assert!(blocks.iter().all(|b| b["type"] != "video"));
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn oversized_and_empty_pdf_remain_references_without_inline_payloads() {
    let dir = std::env::temp_dir().join(format!("ferrite-media-limit-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let large = dir.join("large.pdf");
    fs::File::create(&large)
        .unwrap()
        .set_len(5 * 1024 * 1024 + 1)
        .unwrap();
    let empty = dir.join("empty.pdf");
    fs::write(&empty, []).unwrap();
    let mut replay = Replay::new("claude", vec![]);
    replay.drain();
    replay
        .session
        .send(&ferrite_core::prompt_files::compose(
            "Read",
            &[large, empty],
        ))
        .unwrap();
    let frame = replay.wait_host(|v| v["type"] == "user");
    let blocks = frame["message"]["content"].as_array().unwrap();
    assert_eq!(blocks.len(), 1);
    assert!(blocks[0]["text"].as_str().unwrap().contains("large.pdf"));
    assert!(blocks[0]["text"].as_str().unwrap().contains("empty.pdf"));
    fs::remove_dir_all(dir).unwrap();
}
