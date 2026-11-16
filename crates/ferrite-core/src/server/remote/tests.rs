use super::*;

#[test]
fn profile_publication_does_not_replace_an_existing_environment() {
    let directory = std::env::temp_dir().join(format!("ferrite-profile-{}", random_secret(8)));
    let path = directory.join("link.json");
    write_private_new(&path, b"first environment").unwrap();
    assert_eq!(
        write_private_new(&path, b"second environment")
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"first environment");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(all(unix, feature = "test-support"))]
mod network {
    use super::*;
    use crate::machine::remote::{self, Profile};
    use tokio_tungstenite::tungstenite::Message as ClientFrame;

    struct NoSessions;
    impl crate::engine::Spawner for NoSessions {
        fn spawn(
            &mut self,
            _: crate::engine::SpawnRequest,
        ) -> io::Result<Box<dyn crate::providers::Session>> {
            Err(io::Error::other("This fixture does not spawn agents."))
        }
    }

    struct Fixture {
        folder: PathBuf,
        gateway: Gateway,
        profile: Profile,
        granted: Granted,
        stop: Option<tokio::sync::oneshot::Sender<()>>,
    }
    impl Fixture {
        fn new() -> Self {
            let folder = std::env::temp_dir().join(format!("ferrite-link-{}", random_secret(8)));
            let store = folder.join("remote-store");
            let _dial =
                crate::server::fixture_machine(store.clone(), Box::new(NoSessions)).unwrap();
            let mut access = Access::open(&store).unwrap();
            let code = access.issue().unwrap();
            let granted = access.exchange(&code, "test desktop").unwrap();
            let gateway = Gateway {
                access: Arc::new(Mutex::new(access)),
                socket: link::address(&store),
                slots: Arc::new(Semaphore::new(32)),
            };
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let address = listener.local_addr().unwrap();
            let profile = Profile {
                origin: format!("http://{address}"),
                server_id: granted.server_id.clone(),
                credential: "test-not-persisted".into(),
            };
            let (stop, stopped) = tokio::sync::oneshot::channel();
            let served = gateway.clone();
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(async {
                    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                    axum::serve(listener, app(served))
                        .with_graceful_shutdown(async {
                            let _ = stopped.await;
                        })
                        .await
                        .unwrap();
                });
            });
            Self {
                folder,
                gateway,
                profile,
                granted,
                stop: Some(stop),
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Some(stop) = self.stop.take() {
                let _ = stop.send(());
            }
            let _ = std::fs::remove_dir_all(&self.folder);
        }
    }

    #[test]
    fn authenticated_link_attaches_to_the_real_server_and_rejects_identity_change() {
        let fixture = Fixture::new();
        remote::verify_connection(&fixture.profile, &fixture.granted.secret).unwrap();
        remote::verify_connection(&fixture.profile, &fixture.granted.secret).unwrap();
        let mut changed = fixture.profile.clone();
        changed.server_id = "00000000000000000000000000000000".into();
        assert!(remote::verify_connection(&changed, &fixture.granted.secret).is_err());
        let secure = Profile {
            origin: fixture.profile.origin.replacen("http", "https", 1),
            ..fixture.profile.clone()
        };
        assert!(remote::socket::runtime()
            .unwrap()
            .block_on(remote::socket::connect(
                &secure,
                &fixture.granted.secret,
                "/v1/link"
            ))
            .is_err());
    }

    #[test]
    fn revocation_closes_live_links_and_blocks_reconnect() {
        let fixture = Fixture::new();
        remote::socket::runtime().unwrap().block_on(async {
            let mut socket =
                remote::socket::connect(&fixture.profile, &fixture.granted.secret, "/v1/link")
                    .await
                    .unwrap();
            fixture
                .gateway
                .access
                .lock()
                .unwrap()
                .revoke(&fixture.granted.session_id)
                .unwrap();
            let end = tokio::time::timeout(Duration::from_secs(3), socket.next())
                .await
                .unwrap();
            assert!(matches!(
                end,
                None | Some(Ok(ClientFrame::Close(_))) | Some(Err(_))
            ));
            assert!(
                remote::socket::connect(&fixture.profile, &fixture.granted.secret, "/v1/link")
                    .await
                    .is_err()
            );
            let unauthenticated = fixture.profile.origin.replacen("http", "ws", 1) + "/v1/link";
            assert!(tokio_tungstenite::connect_async(unauthenticated)
                .await
                .is_err());
        });
    }

    #[test]
    fn remote_terminal_runs_in_remote_directory_resizes_and_cleans_up() {
        let fixture = Fixture::new();
        let directory = fixture.folder.join("remote-only-workspace");
        std::fs::create_dir_all(&directory).unwrap();
        remote::socket::runtime().unwrap().block_on(async {
            let mut socket =
                remote::socket::connect(&fixture.profile, &fixture.granted.secret, "/v1/terminal")
                    .await
                    .unwrap();
            socket
                .send(ClientFrame::Text(
                    serde_json::to_string(&remote::terminal::Command::Open {
                        cwd: directory.to_string_lossy().into_owned(),
                        columns: 80,
                        rows: 24,
                    })
                    .unwrap()
                    .into(),
                ))
                .await
                .unwrap();
            socket
                .send(ClientFrame::Text(
                    serde_json::to_string(&remote::terminal::Command::Resize {
                        columns: 101,
                        rows: 37,
                    })
                    .unwrap()
                    .into(),
                ))
                .await
                .unwrap();
            socket
                .send(ClientFrame::Binary(
                    b"printf 'REMOTE_CWD='; pwd; stty size; printf 'REMOTE_PID=%s\\n' $$\n"
                        .to_vec()
                        .into(),
                ))
                .await
                .unwrap();
            let mut output = String::new();
            tokio::time::timeout(Duration::from_secs(5), async {
                while !(output.contains(&format!("REMOTE_CWD={}", directory.display()))
                    && output.contains("37 101")
                    && output.lines().any(|line| {
                        line.starts_with("REMOTE_PID=") && line[11..].trim().parse::<i32>().is_ok()
                    }))
                {
                    match socket.next().await.unwrap().unwrap() {
                        ClientFrame::Binary(bytes) => {
                            output.push_str(&String::from_utf8_lossy(&bytes))
                        }
                        frame => panic!("Unexpected terminal frame {frame:?}"),
                    }
                }
            })
            .await
            .unwrap();
            let pid = output
                .lines()
                .find_map(|line| line.strip_prefix("REMOTE_PID=")?.trim().parse::<i32>().ok())
                .unwrap();
            fixture
                .gateway
                .access
                .lock()
                .unwrap()
                .revoke(&fixture.granted.session_id)
                .unwrap();
            let _ = tokio::time::timeout(Duration::from_secs(3), async {
                while let Some(Ok(frame)) = socket.next().await {
                    if matches!(frame, ClientFrame::Close(_)) {
                        break;
                    }
                }
            })
            .await
            .unwrap();
            tokio::time::timeout(Duration::from_secs(3), async {
                while unsafe { libc::kill(pid, 0) } == 0 {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .unwrap();
        });
    }
}
