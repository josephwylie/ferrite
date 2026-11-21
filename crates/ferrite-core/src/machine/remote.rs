//! Authenticated link routes. Settings hold only the stable environment id.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::Duration;

use super::{
    setup::{Event, Phase},
    Bridge, Machine, Tunnel,
};
use crate::{
    link,
    protocol::{self, ClientMessage, Peer, ServerMessage},
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use url::Url;

pub mod socket;
pub mod terminal;
pub use terminal::dispatch as terminal_dispatch;

pub const FRAME_LIMIT: usize = 256 * 1024;
pub(crate) const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct PairingFields {
    pub host: String,
    pub pairing_code: String,
    expected: Option<String>,
}

impl PairingFields {
    pub fn parse(host: &str, code: &str) -> Result<Self, String> {
        let host = host.trim();
        if host.is_empty() {
            return Err("Enter the remote host.".into());
        }
        let mut url = Url::parse(&if host.contains("://") {
            host.into()
        } else {
            format!("https://{host}")
        })
        .map_err(|_| "Enter a valid remote host or pairing URL.")?;
        let mut code = code.trim().to_string();
        let mut expected = None;
        if let Some(fragment) = url.fragment() {
            for (key, value) in url::form_urlencoded::parse(fragment.as_bytes()) {
                if key == "token" || key == "code" {
                    code = value.into_owned();
                } else if key == "server" {
                    expected = Some(value.into_owned());
                }
            }
        }
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || !matches!(url.path(), "" | "/" | "/pair")
        {
            return Err("Use the host address without a username, query, or path.".into());
        }
        let loopback = match url.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            _ => false,
        };
        if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
            return Err("Remote links require HTTPS. Explicit HTTP is supported only at a loopback IP address.".into());
        }
        if code.len() > 256
            || code.is_empty()
            || !code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        {
            return Err("Enter the pairing code from the remote server.".into());
        }
        if let Some(id) = &expected {
            check_identity(id)?;
        }
        url.set_path("");
        url.set_fragment(None);
        Ok(Self {
            host: url.as_str().trim_end_matches('/').into(),
            pairing_code: code,
            expected,
        })
    }
}

pub(crate) fn check_identity(id: &str) -> Result<(), String> {
    if id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("The remote environment identity is invalid.".into())
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    pub origin: String,
    pub server_id: String,
    pub credential: String,
}

#[derive(Serialize, Deserialize)]
pub struct Descriptor {
    pub server_id: String,
    pub name: String,
}
#[derive(Serialize, Deserialize)]
pub struct Exchange {
    pub code: String,
    pub label: String,
}
#[derive(Serialize, Deserialize)]
pub struct Granted {
    pub server_id: String,
    pub session_id: String,
    pub secret: String,
}

fn profile_path(settings: &Path, target: &str) -> PathBuf {
    super::dir(settings, target).join("link.json")
}

pub(crate) fn profile(settings: &Path, target: &str) -> io::Result<Profile> {
    let profile: Profile = serde_json::from_slice(&std::fs::read(profile_path(settings, target))?)?;
    let parsed = PairingFields::parse(&profile.origin, "validation").map_err(io::Error::other)?;
    if parsed.host != profile.origin || target != format!("ferrite-link:{}", profile.server_id) {
        return Err(io::Error::other(
            "The saved remote environment has changed. Remove it and pair again.",
        ));
    }
    check_identity(&profile.server_id).map_err(io::Error::other)?;
    Ok(profile)
}

pub(super) fn display_target(machine: &Machine, settings: &Path) -> String {
    if machine.is_link() {
        profile(settings, &machine.target)
            .map(|p| p.origin)
            .unwrap_or_else(|_| "Remote link".into())
    } else {
        machine.target.clone()
    }
}

fn credential(profile: &Profile) -> io::Result<keyring::Entry> {
    keyring::Entry::new("app.ferrite.remote", &profile.credential).map_err(|_| {
        io::Error::other("The system credential store is unavailable. Unlock it and try again.")
    })
}

pub(crate) fn secret(profile: &Profile) -> io::Result<String> {
    credential(profile)?.get_password().map_err(|_| io::Error::other("The saved remote credential is unavailable. Unlock the system credential store or pair again."))
}

pub fn configure_tls() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn agent() -> ureq::Agent {
    configure_tls();
    ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .build()
        .into()
}

fn json_body<T: serde::de::DeserializeOwned>(
    mut response: ureq::http::Response<ureq::Body>,
) -> io::Result<T> {
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(16 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 {
        return Err(io::Error::other(
            "The remote server returned an oversized response.",
        ));
    }
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

pub(crate) fn descriptor(origin: &str) -> io::Result<Descriptor> {
    let response = agent().get(format!("{origin}/.well-known/ferrite/environment")).call()
        .map_err(|_| io::Error::other("Could not reach the remote server securely. Check its host, certificate, and network."))?;
    let descriptor: Descriptor = json_body(response)?;
    check_identity(&descriptor.server_id).map_err(io::Error::other)?;
    Ok(descriptor)
}

fn verify_identity(profile: &Profile) -> io::Result<()> {
    if descriptor(&profile.origin)?.server_id != profile.server_id {
        return Err(io::Error::other("The remote host now identifies a different environment. Pair it again before connecting."));
    }
    Ok(())
}

pub(super) fn bridge(machine: &Machine, settings: &Path) -> Bridge {
    let (settings, target) = (settings.to_owned(), machine.target.clone());
    let mut bridge = Bridge::new(
        &machine.name,
        super::dir(&settings, &target).join(format!("bridge-{}.sock", std::process::id())),
        dial(machine, &settings, "/v1/link"),
    );
    bridge.remote = Some((target, settings));
    bridge
}

pub(super) fn dial(machine: &Machine, settings: &Path, path: &'static str) -> super::Dial {
    let (settings, target) = (settings.to_owned(), machine.target.clone());
    Box::new(move || {
        let profile = profile(&settings, &target)?;
        verify_identity(&profile)?;
        socket::tunnel(&profile, &secret(&profile)?, path)
    })
}

pub struct PairAttempt {
    events: mpsc::Receiver<Event>,
    cancelled: Arc<AtomicBool>,
    pending: Arc<Mutex<Option<Machine>>>,
    settings: PathBuf,
}

impl PairAttempt {
    pub fn start(fields: PairingFields, settings: PathBuf) -> io::Result<Self> {
        let (send, events) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stopped = cancelled.clone();
        let pending = Arc::new(Mutex::new(None));
        let registration = pending.clone();
        let worker_settings = settings.clone();
        std::thread::Builder::new()
            .name("ferrite-pair".into())
            .spawn(move || {
                let _ = send.send(Event::Progress(Phase::Inspecting));
                let result = pair(fields, &worker_settings, &stopped, &send);
                let mut pending = registration.lock().unwrap_or_else(|e| e.into_inner());
                match result {
                    Ok(machine) if stopped.load(Ordering::Acquire) => {
                        drop(pending);
                        cleanup_pairing(&machine, &worker_settings);
                    }
                    Ok(machine) => {
                        *pending = Some(machine.clone());
                        let _ = send.send(Event::Ready(machine));
                    }
                    Err(error) if !stopped.load(Ordering::Acquire) => {
                        let _ = send.send(Event::Failed(error.to_string()));
                    }
                    Err(_) => {}
                }
            })?;
        Ok(Self {
            events,
            cancelled,
            pending,
            settings,
        })
    }

    pub fn reconnect(machine: Machine, settings: PathBuf) -> io::Result<Self> {
        let (send, events) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stopped = cancelled.clone();
        let worker_settings = settings.clone();
        std::thread::Builder::new()
            .name("ferrite-link-verify".into())
            .spawn(move || {
                let _ = send.send(Event::Progress(Phase::Verifying));
                let result = (|| {
                    let profile = profile(&worker_settings, &machine.target)?;
                    verify_identity(&profile)?;
                    cancelled_check(&stopped)?;
                    verify_connection(&profile, &secret(&profile)?)
                })();
                if !stopped.load(Ordering::Acquire) {
                    let _ = send.send(match result {
                        Ok(()) => Event::Ready(machine),
                        Err(error) => Event::Failed(error.to_string()),
                    });
                }
            })?;
        Ok(Self {
            events,
            cancelled,
            pending: Arc::new(Mutex::new(None)),
            settings,
        })
    }

    pub fn poll(&mut self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    pub fn commit(&mut self) {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
}

impl Drop for PairAttempt {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        let pending = self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(machine) = pending {
            let settings = self.settings.clone();
            std::thread::spawn(move || cleanup_pairing(&machine, &settings));
        }
    }
}

pub fn forget(machine: &Machine, settings: &Path) -> io::Result<()> {
    if !machine.is_link() {
        return Ok(());
    }
    let saved = profile(settings, &machine.target)?;
    match credential(&saved)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(_) => {
            return Err(io::Error::other(
                "Could not remove the saved remote credential from the system credential store.",
            ))
        }
    }
    std::fs::remove_file(profile_path(settings, &machine.target))
}

fn cleanup_pairing(machine: &Machine, settings: &Path) {
    if let Ok(profile) = profile(settings, &machine.target) {
        if let Ok(secret) = secret(&profile) {
            let _ = agent()
                .post(format!("{}/v1/logout", profile.origin))
                .header("Authorization", format!("Bearer {secret}"))
                .send_empty();
        }
    }
    let _ = forget(machine, settings);
}

fn pair(
    fields: PairingFields,
    settings: &Path,
    cancelled: &AtomicBool,
    send: &mpsc::Sender<Event>,
) -> io::Result<Machine> {
    let descriptor = descriptor(&fields.host)?;
    if fields
        .expected
        .as_ref()
        .is_some_and(|id| *id != descriptor.server_id)
    {
        return Err(io::Error::other(
            "This pairing link belongs to a different environment.",
        ));
    }
    let target = format!("ferrite-link:{}", descriptor.server_id);
    if profile_path(settings, &target).exists() {
        return Err(io::Error::other(
            "This environment is already paired. Remove its saved connection before pairing again.",
        ));
    }
    let entry_ref = format!(
        "{}-{}",
        descriptor.server_id,
        random_secret(16)
    );
    let profile = Profile {
        origin: fields.host,
        server_id: descriptor.server_id.clone(),
        credential: entry_ref,
    };
    let entry = credential(&profile)?;
    cancelled_check(cancelled)?;
    let response = agent().post(format!("{}/v1/pair", profile.origin)).header("Content-Type", "application/json")
        .send(serde_json::to_vec(&Exchange { code: fields.pairing_code, label: "Ferrite desktop".into() })?)
        .map_err(|_| io::Error::other("Pairing failed. The code may be expired or already used. Create a fresh code on the remote server."))?;
    let granted: Granted = json_body(response)?;
    if granted.server_id != profile.server_id {
        return Err(io::Error::other(
            "The remote environment identity changed during pairing.",
        ));
    }
    let _ = send.send(Event::Progress(Phase::Verifying));
    let result = (|| {
        cancelled_check(cancelled)?;
        verify_connection(&profile, &granted.secret)?;
        cancelled_check(cancelled)?;
        entry.set_password(&granted.secret).map_err(|_| {
            io::Error::other("Could not save the remote credential in the system credential store.")
        })?;
        cancelled_check(cancelled)?;
        write_private_new(
            &profile_path(settings, &target),
            &serde_json::to_vec(&profile)?,
        )?;
        Ok(Machine {
            name: descriptor.name,
            target,
            enabled: true,
        })
    })();
    if result.is_err() {
        let _ = entry.delete_credential();
        let _ = agent()
            .post(format!("{}/v1/logout", profile.origin))
            .header("Authorization", format!("Bearer {}", granted.secret))
            .send_empty();
    }
    result
}

fn cancelled_check(cancelled: &AtomicBool) -> io::Result<()> {
    if cancelled.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Connection cancelled.",
        ))
    } else {
        Ok(())
    }
}

pub fn verify_connection(profile: &Profile, secret: &str) -> io::Result<()> {
    let Tunnel {
        from,
        mut to,
        ended,
    } = socket::tunnel(profile, secret, "/v1/link")?;
    link::send(
        &mut to,
        &ClientMessage::Hello {
            generation: protocol::GENERATION,
            client: Peer {
                name: "Ferrite link setup".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            capabilities: Vec::new(),
        },
    )?;
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut lines = link::Lines::new(from);
        let result = match lines.next_message::<ServerMessage>() {
            Ok(Some(ServerMessage::Welcome {
                generation,
                error: None,
                ..
            })) if generation == protocol::GENERATION => {
                match lines.next_message::<ServerMessage>() {
                    Ok(Some(ServerMessage::Snapshot(_))) => Ok(()),
                    _ => Err(io::Error::other(
                        "The remote server did not send its initial state.",
                    )),
                }
            }
            _ => Err(io::Error::other(
                "The remote server did not send a compatible Ferrite welcome.",
            )),
        };
        let _ = send.send(result);
    });
    let result = receive
        .recv_timeout(TIMEOUT)
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Remote verification timed out."))?;
    drop(to);
    ended();
    result
}

pub fn random_secret(bytes: usize) -> String {
    let mut secret = vec![0; bytes];
    rand::rngs::OsRng.fill_bytes(&mut secret);
    secret.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    persist_private(path, bytes, true)
}

pub fn write_private_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    persist_private(path, bytes, false)
}

fn persist_private(path: &Path, bytes: &[u8], replace: bool) -> io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing private directory."))?;
    std::fs::create_dir_all(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
    }
    let temporary = directory.join(format!(".remote-{}", random_secret(8)));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        if replace {
            std::fs::rename(&temporary, path)?;
        } else {
            std::fs::hard_link(&temporary, path)?;
            std::fs::remove_file(&temporary)?;
        }
        #[cfg(unix)]
        std::fs::File::open(directory)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn api_dial_for_a_link_requires_its_saved_profile_before_connecting() {
        let settings = std::env::temp_dir().join(format!(
            "ferrite-api-route-{}",
            random_secret(8)
        ));
        let machine = Machine::new("ferrite-link:00112233445566778899aabbccddeeff");
        match super::super::api_dial(&machine, &settings)() {
            Err(error) => assert_eq!(error.kind(), io::ErrorKind::NotFound),
            Ok(_) => panic!("a link must load its profile, never launch an SSH tunnel"),
        }
    }

    #[test]
    fn links_require_tls_except_explicit_loopback_and_keep_secrets_out_of_origins() {
        assert!(PairingFields::parse("http://example.com", "abc").is_err());
        assert!(PairingFields::parse("http://localhost", "abc").is_err());
        assert!(PairingFields::parse("https://user:pass@example.com", "abc").is_err());
        assert!(PairingFields::parse("https://example.com?token=abc", "abc").is_err());
        let fields = PairingFields::parse(
            "http://127.0.0.1:7000/pair#token=abc&server=00112233445566778899aabbccddeeff",
            "",
        )
        .unwrap();
        assert_eq!(fields.host, "http://127.0.0.1:7000");
        assert_eq!(fields.pairing_code, "abc");
        assert_eq!(
            PairingFields::parse("example.com", "abc").unwrap().host,
            "https://example.com"
        );
    }
    #[test]
    fn legacy_machines_remain_readable_and_link_targets_are_validated() {
        let machine: Machine =
            serde_json::from_str(r#"{"name":"host","target":"user@host"}"#).unwrap();
        assert!(machine.enabled);
        assert!(!machine.is_link());
        assert!(
            super::super::check_target("ferrite-link:00112233445566778899aabbccddeeff").is_ok()
        );
        assert!(super::super::check_target("ferrite-link:secret").is_err());
    }
}
