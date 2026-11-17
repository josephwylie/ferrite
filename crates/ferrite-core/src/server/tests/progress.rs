use super::*;

#[test]
fn a_conversation_progresses_before_incoming_traffic_is_drained() {
    let dir = scratch("intake-progress");
    let (spawner, providers) = spawn();
    let mut engine = engine(&dir.join("threads"), spawner);
    let busy = open_in(&mut engine, &dir);
    let _busy_provider = providers.recv().unwrap();
    let other = open_in(&mut engine, &dir);
    let other_provider = providers.recv().unwrap();
    engine.pump();
    other_provider
        .send(text("another conversation advanced"))
        .unwrap();

    let (_, renders) = render::remote();
    let server = Server::new(
        engine,
        ServerInfo {
            version: "0.9.0".into(),
            executable: "/bin/ferrite".into(),
        },
        renders,
        Arc::new(Mutex::new(SessionDefaults::default())),
        dir.clone(),
    );
    let (heard, hear) = mpsc::channel();
    let (answer, answers) = mpsc::channel();
    heard
        .send(Heard::Api(crate::api::Heard::Joined(1, answer)))
        .unwrap();
    for index in 0..513 {
        let thread = if index == 256 { other } else { busy };
        heard
            .send(Heard::Api(crate::api::Heard::Request(
                1,
                crate::api::Request {
                    id: index.to_string(),
                    method: crate::api::Method::AgentRead(crate::api::schema::AgentReadParams {
                        thread,
                        lines: None,
                    }),
                    caller: None,
                },
            )))
            .unwrap();
    }
    drop(heard);

    assert_eq!(
        server.serve(hear),
        1,
        "the listener closed after the queued traffic"
    );
    let replies: Vec<serde_json::Value> = answers
        .try_iter()
        .map(|line| serde_json::from_slice(&line).unwrap())
        .collect();
    assert_eq!(replies.len(), 513);
    assert_eq!(replies[256]["id"], "256");
    assert!(
        replies[256]["result"]["text"]
            .as_str()
            .unwrap()
            .contains("another conversation advanced"),
        "the other conversation must advance before the remaining traffic: {:?}",
        replies[256]
    );
    assert_eq!(replies.last().unwrap()["id"], "512");
    let _ = std::fs::remove_dir_all(dir);
}
