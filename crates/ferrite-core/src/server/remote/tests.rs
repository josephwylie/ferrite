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
            Self::with_tls(None)
        }

        fn with_tls(tls: Option<axum_server::tls_rustls::RustlsConfig>) -> Self {
            let folder = std::env::temp_dir().join(format!("ferrite-link-{}", random_secret(8)));
            let store = folder.join("remote-store");
            let (heard, hear) = std::sync::mpsc::channel();
            link::accept_all(
                link::listen(&link::address(&store)).unwrap(),
                heard.clone(),
                link::LAGGED_BYTES,
            )
            .unwrap();
            crate::api::accept_all(link::listen(&crate::api::address(&store)).unwrap(), heard)
                .unwrap();
            let server_store = store.clone();
            std::thread::spawn(move || {
                let engine = crate::engine::Engine::try_new(
                    crate::store::Store::open(&server_store).unwrap(),
                    Box::new(NoSessions),
                )
                .unwrap();
                let info = crate::protocol::ServerInfo {
                    version: env!("CARGO_PKG_VERSION").into(),
                    executable: PathBuf::new(),
                };
                let (_, renders) = crate::server::render::remote();
                crate::server::Server::new(
                    engine,
                    info,
                    renders,
                    Arc::default(),
                    server_store.parent().unwrap().to_path_buf(),
                )
                .serve(hear);
            });
            let mut access = Access::open(&store).unwrap();
            let code = access.issue().unwrap();
            let granted = access.exchange(&code, "test desktop").unwrap();
            let gateway = Gateway {
                access: Arc::new(Mutex::new(access)),
                socket: link::address(&store),
                api_socket: crate::api::address(&store),
                slots: Arc::new(Semaphore::new(32)),
            };
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let address = listener.local_addr().unwrap();
            let profile = Profile {
                origin: format!(
                    "{}://{address}",
                    if tls.is_some() { "https" } else { "http" }
                ),
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
                    if let Some(tls) = tls {
                        let handle = axum_server::Handle::new();
                        let server = axum_server::from_tcp_rustls(listener, tls)
                            .handle(handle.clone())
                            .serve(app(served).into_make_service());
                        tokio::pin!(server);
                        tokio::select! {
                            result = &mut server => result.unwrap(),
                            _ = stopped => {
                                handle.shutdown();
                                server.await.unwrap();
                            },
                        }
                    } else {
                        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                        axum::serve(listener, app(served))
                            .with_graceful_shutdown(async {
                                let _ = stopped.await;
                            })
                            .await
                            .unwrap();
                    }
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

    mod tls {
        use super::*;
        use crate::protocol::{self, ClientMessage, Peer, ServerMessage};
        use rustls::pki_types::{pem::PemObject, CertificateDer};
        use tokio_tungstenite::tungstenite::{
            client::IntoClientRequest, handshake::client::Response, Error,
        };

        const CA: &[u8] = include_bytes!("fixtures/test-ca.pem");

        fn fixture() -> Fixture {
            remote::configure_tls();
            let tls = remote::socket::runtime()
                .unwrap()
                .block_on(axum_server::tls_rustls::RustlsConfig::from_pem(
                    include_bytes!("fixtures/test-server.pem").to_vec(),
                    include_bytes!("fixtures/test-server-key.pem").to_vec(),
                ))
                .unwrap();
            Fixture::with_tls(Some(tls))
        }

        fn roots() -> rustls::RootCertStore {
            let mut roots = rustls::RootCertStore::empty();
            roots
                .add(CertificateDer::from_pem_slice(CA).unwrap())
                .unwrap();
            roots
        }

        async fn connect(
            fixture: &Fixture,
            hostname: &str,
            secret: &str,
            roots: rustls::RootCertStore,
        ) -> Result<(remote::socket::Socket, Response), Error> {
            let address = fixture.profile.origin.strip_prefix("https://").unwrap();
            let port = address.parse::<SocketAddr>().unwrap().port();
            let mut request = format!("wss://{hostname}:{port}/v1/link")
                .into_client_request()
                .unwrap();
            request
                .headers_mut()
                .insert("Authorization", format!("Bearer {secret}").parse().unwrap());
            let config = rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            let stream = tokio::net::TcpStream::connect(address).await.unwrap();
            tokio::time::timeout(
                Duration::from_secs(3),
                tokio_tungstenite::client_async_tls_with_config(
                    request,
                    stream,
                    None,
                    Some(tokio_tungstenite::Connector::Rustls(Arc::new(config))),
                ),
            )
            .await
            .expect("the TLS handshake completes")
        }

        #[test]
        fn trusted_certificate_pairs_and_attaches_to_the_real_server() {
            let fixture = fixture();
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .tls_config(
                    ureq::tls::TlsConfig::builder()
                        .root_certs(vec![ureq::tls::Certificate::from_pem(CA).unwrap()].into())
                        .build(),
                )
                .timeout_global(Some(Duration::from_secs(3)))
                .max_redirects(0)
                .build()
                .into();
            let descriptor: Descriptor = serde_json::from_slice(
                &agent
                    .get(format!(
                        "{}/.well-known/ferrite/environment",
                        fixture.profile.origin
                    ))
                    .call()
                    .unwrap()
                    .body_mut()
                    .read_to_vec()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(descriptor.server_id, fixture.profile.server_id);
            let code = fixture.gateway.access.lock().unwrap().issue().unwrap();
            let granted: Granted = serde_json::from_slice(
                &agent
                    .post(format!("{}/v1/pair", fixture.profile.origin))
                    .header("Content-Type", "application/json")
                    .send(
                        serde_json::to_vec(&Exchange {
                            code,
                            label: "TLS desktop".into(),
                        })
                        .unwrap(),
                    )
                    .unwrap()
                    .body_mut()
                    .read_to_vec()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(granted.server_id, descriptor.server_id);

            remote::socket::runtime().unwrap().block_on(async {
                let denied = connect(&fixture, "127.0.0.1", "incorrect", roots())
                    .await
                    .unwrap_err();
                assert!(
                    matches!(denied, Error::Http(response) if response.status() == StatusCode::UNAUTHORIZED)
                );
                let (mut socket, response) = connect(&fixture, "127.0.0.1", &granted.secret, roots())
                    .await
                    .unwrap();
                assert_eq!(response.headers()["x-ferrite-environment"], granted.server_id);
                let mut hello = Vec::new();
                link::send(
                    &mut hello,
                    &ClientMessage::Hello {
                        generation: protocol::GENERATION,
                        client: Peer {
                            name: "TLS fixture".into(),
                            version: env!("CARGO_PKG_VERSION").into(),
                        },
                        capabilities: Vec::new(),
                    },
                )
                .unwrap();
                socket.send(ClientFrame::Binary(hello.into())).await.unwrap();
                let mut bytes = Vec::new();
                tokio::time::timeout(Duration::from_secs(3), async {
                    while bytes.iter().filter(|&&byte| byte == b'\n').count() < 2 {
                        let ClientFrame::Binary(frame) = socket.next().await.unwrap().unwrap() else {
                            panic!("expected the Server welcome and initial state");
                        };
                        bytes.extend_from_slice(&frame);
                        assert!(bytes.len() <= link::MAX_LINE);
                    }
                })
                .await
                .unwrap();
                let mut lines = link::Lines::new(bytes.as_slice());
                assert!(matches!(
                    lines.next_message::<ServerMessage>().unwrap(),
                    Some(ServerMessage::Welcome { generation, error: None, .. })
                        if generation == protocol::GENERATION
                ));
                assert!(matches!(
                    lines.next_message::<ServerMessage>().unwrap(),
                    Some(ServerMessage::Snapshot(_))
                ));
                socket.close(None).await.unwrap();
            });
        }

        #[test]
        fn untrusted_certificate_and_wrong_hostname_are_rejected() {
            let fixture = fixture();
            remote::socket::runtime().unwrap().block_on(async {
                let error = connect(
                    &fixture,
                    "127.0.0.1",
                    &fixture.granted.secret,
                    rustls::RootCertStore::empty(),
                )
                .await
                .unwrap_err();
                let Error::Io(error) = error else {
                    panic!("expected a TLS verification error")
                };
                assert!(matches!(
                    error
                        .get_ref()
                        .and_then(|error| error.downcast_ref::<rustls::Error>()),
                    Some(rustls::Error::InvalidCertificate(
                        rustls::CertificateError::UnknownIssuer
                    ))
                ));

                let error = connect(
                    &fixture,
                    "wrong-host.invalid",
                    &fixture.granted.secret,
                    roots(),
                )
                .await
                .unwrap_err();
                let Error::Io(error) = error else {
                    panic!("expected a TLS verification error")
                };
                assert!(matches!(
                    error
                        .get_ref()
                        .and_then(|error| error.downcast_ref::<rustls::Error>()),
                    Some(rustls::Error::InvalidCertificate(
                        rustls::CertificateError::NotValidForName
                            | rustls::CertificateError::NotValidForNameContext { .. }
                    ))
                ));
            });
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
    fn authenticated_api_routes_to_the_remote_server_and_revokes_live_access() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let fixture = Fixture::new();
        remote::socket::runtime().unwrap().block_on(async {
            let url = fixture.profile.origin.replacen("http", "ws", 1) + "/v1/api";
            assert!(tokio_tungstenite::connect_async(&url).await.is_err());
            assert!(remote::socket::connect(&fixture.profile, "incorrect", "/v1/api")
                .await
                .is_err());
            let mut browser_request = url.into_client_request().unwrap();
            browser_request.headers_mut().insert(
                "authorization",
                format!("Bearer {}", fixture.granted.secret).parse().unwrap(),
            );
            browser_request.headers_mut().insert("origin", "https://example.com".parse().unwrap());
            assert!(tokio_tungstenite::connect_async(browser_request).await.is_err());

            let mut socket = remote::socket::connect(
                &fixture.profile,
                &fixture.granted.secret,
                "/v1/api",
            )
            .await
            .unwrap();
            socket.send(ClientFrame::Binary(
                b"{\"id\":\"remote-api\",\"method\":\"project.list\",\"params\":{}}\n".to_vec().into(),
            )).await.unwrap();
            let response = tokio::time::timeout(Duration::from_secs(3), async {
                let mut bytes = Vec::new();
                loop {
                    match socket.next().await.unwrap().unwrap() {
                        ClientFrame::Binary(part) => {
                            bytes.extend_from_slice(&part);
                            if bytes.contains(&b'\n') {
                                return serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
                            }
                        }
                        ClientFrame::Ping(_) | ClientFrame::Pong(_) => {},
                        frame => panic!("API closed without its response: {frame:?}"),
                    }
                }
            }).await.unwrap();
            assert_eq!(response["id"], "remote-api");
            assert!(response.get("result").is_some(), "{response}");
            assert!(response.get("error").is_none(), "{response}");

            fixture.gateway.access.lock().unwrap()
                .revoke(&fixture.granted.session_id).unwrap();
            let end = tokio::time::timeout(Duration::from_secs(3), socket.next()).await.unwrap();
            assert!(matches!(end, None | Some(Ok(ClientFrame::Close(_))) | Some(Err(_))));
            assert!(remote::socket::connect(&fixture.profile, &fixture.granted.secret, "/v1/api")
                .await.is_err());
        });
    }

    #[test]
    fn fragmented_attachment_upload_crosses_the_gateway_without_dropping_the_link() {
        use crate::attachment_transfer;
        use crate::protocol::{self, ClientMessage, Command, Done, Op, Peer, ServerMessage};

        struct Fragmented(Box<dyn Write + Send>);
        impl Write for Fragmented {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0.write(&bytes[..bytes.len().min(512)])
            }
            fn flush(&mut self) -> io::Result<()> {
                self.0.flush()
            }
        }

        let fixture = Fixture::new();
        let bytes: Vec<u8> = (0..attachment_transfer::CHUNK_SIZE * 3 + 17)
            .map(|index| (index % 251) as u8)
            .collect();
        let source = fixture.folder.join("fragmented.bin");
        std::fs::write(&source, &bytes).unwrap();
        let profile = fixture.profile.clone();
        let secret = fixture.granted.secret.clone();
        let transfer = attachment_transfer::upload(source, move || {
            let tunnel = remote::socket::tunnel(&profile, &secret, "/v1/link")?;
            let mut writer = Fragmented(tunnel.to);
            let mut lines = link::Lines::new(tunnel.from);
            link::send(&mut writer, &ClientMessage::Hello {
                generation: protocol::GENERATION,
                client: Peer { name: "fragmented upload".into(), version: "test".into() },
                capabilities: Vec::new(),
            })?;
            assert!(matches!(lines.next_message::<ServerMessage>()?,
                Some(ServerMessage::Welcome { error: None, .. })));
            let mut id = 0;
            Ok(Box::new(move |request| {
                id += 1;
                link::send(&mut writer, &ClientMessage::Command(Command {
                    id,
                    op: Op::Attachment { request },
                }))?;
                loop {
                    match lines.next_message::<ServerMessage>()? {
                        Some(ServerMessage::Reply { id: replied, ok: Some(Done::Attachment { reply }), error: None })
                            if replied == id => return Ok(reply),
                        Some(ServerMessage::Reply { id: replied, error: Some(error), .. })
                            if replied == id => return Err(io::Error::other(format!("{error:?}"))),
                        Some(_) => {},
                        None => return Err(io::Error::other("the gateway dropped a valid upload")),
                    }
                }
            }) as attachment_transfer::Exchange)
        });
        let retained = transfer.result.recv_timeout(Duration::from_secs(20)).unwrap().unwrap();
        assert_eq!(retained.size, bytes.len() as u64);
        assert!(retained.path.starts_with(fixture.folder.join("remote-store")));
        assert_eq!(std::fs::read(&retained.path).unwrap(), bytes);
        assert_eq!(transfer.progress.try_iter().last().unwrap().sent, retained.size);
        remote::verify_connection(&fixture.profile, &fixture.granted.secret).unwrap();
    }

    #[test]
    fn remote_terminal_backpressures_large_pastes_while_forwarding_output() {
        let fixture = Fixture::new();
        let directory = fixture.folder.canonicalize().unwrap();
        let paste = vec![b'x'; 512 * 1024];
        remote::socket::runtime().unwrap().block_on(async {
            let mut socket = remote::socket::connect(
                &fixture.profile, &fixture.granted.secret, "/v1/terminal",
            ).await.unwrap();
            socket.send(ClientFrame::Text(serde_json::to_string(
                &remote::terminal::Command::Open {
                    cwd: directory.to_string_lossy().into_owned(), columns: 80, rows: 24,
                },
            ).unwrap().into())).await.unwrap();
            socket.send(ClientFrame::Binary(
                b"stty raw -echo; printf '\\nPASTE_READY\\n'; while [ ! -f read-now ]; do printf '\\nPASTE_WAITING\\n'; sleep 0.05; done; dd bs=1 count=524288 of=pasted.bin 2>/dev/null; printf '\\nPASTE_DONE\\n'; exit\n"
                    .to_vec().into(),
            )).await.unwrap();
            let mut output = String::new();
            tokio::time::timeout(Duration::from_secs(5), async {
                while !output.lines().any(|line| line.trim() == "PASTE_READY") {
                    match socket.next().await.unwrap().unwrap() {
                        ClientFrame::Binary(bytes) => output.push_str(&String::from_utf8_lossy(&bytes)),
                        frame => panic!("terminal did not become ready: {frame:?}"),
                    }
                }
            }).await.unwrap();
            for chunk in paste.chunks(4096) {
                socket.feed(ClientFrame::Binary(chunk.to_vec().into())).await.unwrap();
            }
            socket.flush().await.unwrap();
            output.clear();
            tokio::time::timeout(Duration::from_secs(5), async {
                while output.lines().filter(|line| line.trim() == "PASTE_WAITING").count() < 3 {
                    match socket.next().await.unwrap().unwrap() {
                        ClientFrame::Binary(bytes) => output.push_str(&String::from_utf8_lossy(&bytes)),
                        frame => panic!("a full input queue stopped terminal output: {frame:?}"),
                    }
                }
            }).await.unwrap();
            std::fs::write(directory.join("read-now"), b"read").unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    match socket.next().await.unwrap().unwrap() {
                        ClientFrame::Binary(bytes) => output.push_str(&String::from_utf8_lossy(&bytes)),
                        ClientFrame::Text(text) => {
                            let event: serde_json::Value = serde_json::from_str(&text).unwrap();
                            assert_eq!(event["type"], "exit", "{event}");
                            assert_eq!(event["code"], 0, "{event}");
                            break;
                        }
                        frame => panic!("large paste closed the terminal: {frame:?}"),
                    }
                }
            }).await.unwrap();
            assert!(output.lines().any(|line| line.trim() == "PASTE_DONE"));
        });
        assert_eq!(std::fs::read(directory.join("pasted.bin")).unwrap(), paste);
    }

    #[test]
    fn remote_terminal_drains_final_output_before_reporting_exit() {
        let fixture = Fixture::new();
        let directory = fixture.folder.canonicalize().unwrap();
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
                .send(ClientFrame::Binary(
                    b"exec /bin/sh -c 'printf \"%262144s\" \"\" | tr \" \" x; printf \"\\nREMOTE_FINAL_MARKER\\n\"; exit 37'\n"
                        .to_vec()
                        .into(),
                ))
                .await
                .unwrap();
            let mut output = Vec::new();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    match socket.next().await.expect("the terminal reports its exit").unwrap() {
                        ClientFrame::Binary(bytes) => output.extend_from_slice(&bytes),
                        ClientFrame::Text(text) => {
                            let event: serde_json::Value = serde_json::from_str(&text).unwrap();
                            assert_eq!(event["type"], "exit");
                            assert_eq!(event["code"], 37);
                            break;
                        },
                        ClientFrame::Ping(_) | ClientFrame::Pong(_) => {},
                        frame => panic!("terminal closed before reporting its exit: {frame:?}"),
                    }
                }
            })
            .await
            .unwrap();
            let output = String::from_utf8(output).unwrap();
            assert!(output.contains(&"x".repeat(262144)), "terminal output was truncated");
            assert!(output.contains("\r\nREMOTE_FINAL_MARKER\r\n"), "terminal lost its final output");
        });
    }

    #[test]
    fn remote_terminal_runs_in_remote_directory_resizes_and_cleans_up() {
        let fixture = Fixture::new();
        let directory = fixture.folder.join("remote-only-workspace");
        std::fs::create_dir_all(&directory).unwrap();
        let directory = directory.canonicalize().unwrap();
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
