//! HTTPS ingress and local-owner access administration for the existing Server.

use ferrite_core::{
    link::{self, TryClone},
    machine::remote::{
        random_secret, write_private, Descriptor, Exchange, Granted, PairingFields, FRAME_LIMIT,
    },
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        DefaultBodyLimit, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use std::{
    io::{self, Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Semaphore;

mod access;
mod terminal;
use access::Access;

#[derive(Clone)]
struct Gateway {
    access: Arc<Mutex<Access>>,
    socket: PathBuf,
    api_socket: PathBuf,
    slots: Arc<Semaphore>,
    /// The program a terminal runs; `None` is the account's login shell.
    shell: Option<PathBuf>,
}
impl Gateway {
    fn active(&self, session: &str) -> bool {
        self.access
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .active(session)
    }
    fn authorize(&self, headers: &HeaderMap) -> Result<String, StatusCode> {
        if headers.contains_key("origin") {
            return Err(StatusCode::FORBIDDEN);
        }
        let secret = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or(StatusCode::UNAUTHORIZED)?;
        self.access
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .authorize(secret)
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}

async fn descriptor(State(gateway): State<Gateway>) -> Json<Descriptor> {
    Json(
        gateway
            .access
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .descriptor(),
    )
}

async fn pair(
    State(gateway): State<Gateway>,
    headers: HeaderMap,
    Json(request): Json<Exchange>,
) -> Result<Json<Granted>, StatusCode> {
    if headers.contains_key("origin") {
        return Err(StatusCode::FORBIDDEN);
    }
    gateway
        .access
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .exchange(&request.code, &request.label)
        .map(Json)
        .map_err(|_| StatusCode::UNAUTHORIZED)
}

async fn logout(
    State(gateway): State<Gateway>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let session = gateway.authorize(&headers)?;
    gateway
        .access
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .revoke(&session)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn upgrade_link(
    State(gateway): State<Gateway>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgraded(gateway, headers, upgrade, Route::Link)
}
async fn upgrade_terminal(
    State(gateway): State<Gateway>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgraded(gateway, headers, upgrade, Route::Terminal)
}

async fn upgrade_api(
    State(gateway): State<Gateway>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgraded(gateway, headers, upgrade, Route::Api)
}

enum Route {
    Link,
    Api,
    Terminal,
}

fn upgraded(
    gateway: Gateway,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
    route: Route,
) -> Response {
    let session = match gateway.authorize(&headers) {
        Ok(session) => session,
        Err(status) => return status.into_response(),
    };
    let permit = match gateway.slots.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => return StatusCode::TOO_MANY_REQUESTS.into_response(),
    };
    let identity = gateway
        .access
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .descriptor()
        .server_id;
    let mut response = upgrade
        .max_message_size(FRAME_LIMIT)
        .max_frame_size(FRAME_LIMIT)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            match route {
                Route::Terminal => terminal::serve(socket, gateway, session).await,
                Route::Link => {
                    let address = gateway.socket.clone();
                    carry(socket, gateway, session, address).await;
                }
                Route::Api => {
                    let address = gateway.api_socket.clone();
                    carry(socket, gateway, session, address).await;
                }
            }
        })
        .into_response();
    response.headers_mut().insert(
        "x-ferrite-environment",
        identity.parse().expect("hex identity"),
    );
    response
}

async fn send_frame(socket: &mut WebSocket, message: Message) -> io::Result<()> {
    tokio::time::timeout(Duration::from_secs(2), socket.send(message))
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "The remote client stopped reading.",
            )
        })?
        .map_err(io::Error::other)
}

async fn carry(mut websocket: WebSocket, gateway: Gateway, session: String, address: PathBuf) {
    let Ok(mut stream) = link::connect(&address) else {
        return;
    };
    let Ok(mut upload) = stream.try_clone() else {
        return;
    };
    let Ok(control) = stream.try_clone() else {
        return;
    };
    let (incoming, mut input) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
    let (output, mut outgoing) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
    let access = gateway.access.clone();
    let owner = session.clone();
    let upstream = tokio::task::spawn_blocking(move || {
        let mut pending = Vec::new();
        while let Some(data) = input.blocking_recv() {
            pending.extend_from_slice(&data);
            if pending.len() > link::MAX_LINE {
                break;
            }
            let mut consumed = 0;
            while let Some(end) = pending[consumed..].iter().position(|&byte| byte == b'\n') {
                let end = consumed + end + 1;
                let authorized = access
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .active(&owner);
                if !authorized || upload.write_all(&pending[consumed..end]).is_err() {
                    return;
                }
                consumed = end;
            }
            pending.drain(..consumed);
        }
        let _ = upload.shutdown();
    });
    let downstream = tokio::task::spawn_blocking(move || {
        let mut buffer = vec![0; 64 * 1024];
        while let Ok(count) = stream.read(&mut buffer) {
            if count == 0 || output.blocking_send(buffer[..count].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut clock = tokio::time::interval(Duration::from_millis(200));
    let mut pending: Option<(Vec<u8>, std::time::Instant)> = None;
    loop {
        tokio::select! {
            _ = clock.tick() => {
                if !gateway.active(&session)
                    || pending.as_ref().is_some_and(|(_, since)| since.elapsed() > Duration::from_secs(2))
                {
                    break;
                }
            },
            permit = incoming.reserve(), if pending.is_some() => match permit {
                Ok(permit) => permit.send(pending.take().expect("a pending input frame").0),
                Err(_) => break,
            },
            bytes = outgoing.recv() => match bytes {
                Some(bytes) => if send_frame(&mut websocket, Message::Binary(bytes.into())).await.is_err() { break; },
                None => break,
            },
            message = websocket.next(), if pending.is_none() => match message {
                Some(Ok(Message::Binary(bytes))) if gateway.active(&session) => {
                    pending = Some((bytes.to_vec(), std::time::Instant::now()));
                },
                Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {},
                _ => break,
            },
        }
    }
    let _ = control.shutdown();
    drop(incoming);
    drop(outgoing);
    let _ = websocket.close().await;
    let _ = upstream.await;
    let _ = downstream.await;
}

fn admin(gateway: Gateway, listener: link::Listener, public_url: String) -> io::Result<()> {
    std::thread::Builder::new().name("ferrite-access-admin".into()).spawn(move || {
        while let Ok(mut stream) = listener.accept() {
            let result = (|| -> io::Result<serde_json::Value> {
                let mut bytes = Vec::new();
                let mut byte = [0];
                while bytes.len() < 1024 && stream.read(&mut byte)? == 1 {
                    if byte[0] == b'\n' { break; }
                    bytes.push(byte[0]);
                }
                let request: serde_json::Value = serde_json::from_slice(&bytes)?;
                let mut access = gateway.access.lock().unwrap_or_else(|e| e.into_inner());
                match request["command"].as_str() {
                    Some("pair") => {
                        let code = access.issue()?;
                        let id = access.descriptor().server_id;
                        Ok(serde_json::json!({"url":format!("{public_url}/pair#token={code}&server={id}"),"code":code,"server_id":id,"expires_in":600}))
                    }
                    Some("sessions") => Ok(access.sessions()),
                    Some("revoke") => { access.revoke(request["id"].as_str().ok_or_else(|| io::Error::other("Missing session id."))?)?; Ok(serde_json::json!({"revoked":true})) },
                    _ => Err(io::Error::other("Unknown access command.")),
                }
            })();
            let response = match result { Ok(value) => value, Err(error) => serde_json::json!({"error":error.to_string()}) };
            let _ = link::send(&mut stream, &response);
        }
    })?;
    Ok(())
}

fn value(args: &[String], name: &str) -> io::Result<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|at| args.get(at + 1))
        .cloned()
        .ok_or_else(|| io::Error::other(format!("Missing {name}.")))
}

pub fn cli(args: &[String], store: &Path, executable: &Path) -> i32 {
    let result = if args.first().map(String::as_str) == Some("serve") {
        serve(args, store, executable)
    } else {
        administer(args, store)
    };
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("ferrited remote: {error}");
            1
        }
    }
}

fn administer(args: &[String], store: &Path) -> io::Result<()> {
    let request = match (args.first().map(String::as_str), args.get(1).map(String::as_str)) {
        (Some("pairing"), Some("create")) => serde_json::json!({"command":"pair"}),
        (Some("sessions"), Some("list")) => serde_json::json!({"command":"sessions"}),
        (Some("sessions"), Some("revoke")) => serde_json::json!({"command":"revoke","id":args.get(2).ok_or_else(|| io::Error::other("Specify a session id."))?}),
        _ => return Err(io::Error::other("Use remote serve --listen ADDRESS --public-url URL --cert PEM --key PEM, remote pairing create, remote sessions list, or remote sessions revoke ID.")),
    };
    let mut stream = link::connect(&store.join("remote-admin.sock"))?;
    link::send(&mut stream, &request)?;
    let reply = link::Lines::new(stream)
        .next_message::<serde_json::Value>()?
        .ok_or_else(|| io::Error::other("The remote gateway did not answer."))?;
    if let Some(error) = reply.get("error") {
        return Err(io::Error::other(error.to_string()));
    }
    println!("{}", serde_json::to_string_pretty(&reply)?);
    Ok(())
}

fn serve(args: &[String], store: &Path, executable: &Path) -> io::Result<()> {
    ferrite_core::machine::remote::configure_tls();
    let address: SocketAddr = value(args, "--listen")?.parse().map_err(io::Error::other)?;
    let public = PairingFields::parse(&value(args, "--public-url")?, "validation")
        .map_err(io::Error::other)?;
    let plaintext = args.iter().any(|arg| arg == "--allow-loopback-http");
    if plaintext && (!address.ip().is_loopback() || !public.host.starts_with("http://")) {
        return Err(io::Error::other(
            "Plain HTTP requires both an explicit loopback listener and loopback public URL.",
        ));
    }
    if !plaintext && !public.host.starts_with("https://") {
        return Err(io::Error::other("The public URL must use HTTPS."));
    }
    let tls_paths = if plaintext {
        None
    } else {
        Some((value(args, "--cert")?, value(args, "--key")?))
    };
    ferrite_core::server::agent::ensure_running(store, executable)?;
    let admin_listener = link::listen(&store.join("remote-admin.sock"))?;
    let gateway = Gateway {
        access: Arc::new(Mutex::new(Access::open(store)?)),
        socket: link::address(store),
        api_socket: ferrite_core::api::address(store),
        slots: Arc::new(Semaphore::new(32)),
        shell: None,
    };
    admin(gateway.clone(), admin_listener, public.host)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let router = app(gateway);
        if let Some((cert, key)) = tls_paths {
            let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key).await?;
            axum_server::bind_rustls(address, tls)
                .serve(router.into_make_service())
                .await
        } else {
            let listener = tokio::net::TcpListener::bind(address).await?;
            axum::serve(listener, router).await
        }
    })
}

fn app(gateway: Gateway) -> Router {
    let requests = Arc::new(Semaphore::new(64));
    Router::new()
        .route("/.well-known/ferrite/environment", get(descriptor))
        .route("/v1/pair", post(pair))
        .route("/v1/logout", post(logout))
        .route("/v1/link", get(upgrade_link))
        .route("/v1/api", get(upgrade_api))
        .route("/v1/terminal", get(upgrade_terminal))
        .layer(DefaultBodyLimit::max(1024))
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let requests = requests.clone();
                async move {
                    let Ok(_permit) = requests.try_acquire_owned() else {
                        return StatusCode::TOO_MANY_REQUESTS.into_response();
                    };
                    match tokio::time::timeout(Duration::from_secs(10), next.run(request)).await {
                        Ok(response) => response,
                        Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
                    }
                }
            },
        ))
        .with_state(gateway)
}

#[cfg(test)]
mod tests;
