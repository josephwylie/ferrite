use super::{random_secret, write_private};
use ferrite_core::machine::remote::{Descriptor, Granted};
use ferrite_core::server::ServerId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;

const GRANT_TTL: u64 = 600;
const SESSION_TTL: u64 = 90 * 24 * 60 * 60;

#[derive(Clone, Serialize, Deserialize)]
struct Grant {
    hash: Vec<u8>,
    expires: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct Session {
    id: String,
    label: String,
    hash: Vec<u8>,
    expires: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct Saved {
    name: String,
    grants: Vec<Grant>,
    sessions: Vec<Session>,
}

pub(super) struct Access {
    path: PathBuf,
    /// The Server's id (`crate::server_id`): remote access keeps none of its own.
    id: ServerId,
    saved: Saved,
    recent: Vec<u64>,
}

pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn digest(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}
fn matches(hash: &[u8], secret: &str) -> bool {
    bool::from(hash.ct_eq(&digest(secret)))
}

impl Access {
    pub fn open(store: &Path) -> io::Result<Self> {
        let id = crate::server_id::open(store)?;
        let path = store.join("remote-access.json");
        let saved = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Saved {
                name: std::env::var("HOSTNAME")
                    .or_else(|_| std::env::var("COMPUTERNAME"))
                    .unwrap_or_else(|_| "Remote environment".into()),
                grants: Vec::new(),
                sessions: Vec::new(),
            },
            Err(error) => return Err(error),
        };
        let access = Self {
            path,
            id,
            saved,
            recent: Vec::new(),
        };
        access.persist(&access.saved)?;
        Ok(access)
    }
    fn persist(&self, saved: &Saved) -> io::Result<()> {
        write_private(&self.path, &serde_json::to_vec(saved)?)
    }
    pub fn descriptor(&self) -> Descriptor {
        Descriptor {
            server_id: self.id.as_str().into(),
            name: self.saved.name.clone(),
        }
    }
    pub fn issue(&mut self) -> io::Result<String> {
        let code = random_secret(16);
        let mut saved = self.saved.clone();
        saved.grants.retain(|grant| grant.expires > now());
        if saved.grants.len() >= 16 {
            return Err(io::Error::other(
                "Too many active pairing codes. Wait for one to expire.",
            ));
        }
        saved.grants.push(Grant {
            hash: digest(&code),
            expires: now() + GRANT_TTL,
        });
        self.persist(&saved)?;
        self.saved = saved;
        Ok(code)
    }
    pub fn exchange(&mut self, code: &str, label: &str) -> io::Result<Granted> {
        let now = now();
        self.recent.retain(|time| now.saturating_sub(*time) < 60);
        if self.recent.len() >= 20 {
            return Err(io::Error::other(
                "Pairing is rate limited. Try again in a minute.",
            ));
        }
        self.recent.push(now);
        if code.len() > 256 || label.len() > 80 {
            return Err(io::Error::other("Invalid pairing request."));
        }
        let index = self
            .saved
            .grants
            .iter()
            .position(|grant| grant.expires > now && matches(&grant.hash, code))
            .ok_or_else(|| {
                io::Error::other("Pairing code is invalid, expired, or already used.")
            })?;
        let mut saved = self.saved.clone();
        saved.grants.remove(index);
        saved.sessions.retain(|session| session.expires > now);
        if saved.sessions.len() >= 64 {
            return Err(io::Error::other(
                "Revoke an old session before pairing another client.",
            ));
        }
        let secret = random_secret(32);
        let id = random_secret(16);
        saved.sessions.push(Session {
            id: id.clone(),
            label: label.into(),
            hash: digest(&secret),
            expires: now + SESSION_TTL,
        });
        self.persist(&saved)?;
        self.saved = saved;
        Ok(Granted {
            server_id: self.id.as_str().into(),
            session_id: id,
            secret,
            expires: Some(now + SESSION_TTL),
        })
    }
    pub fn authorize(&self, secret: &str) -> Option<String> {
        if secret.len() != 64 {
            return None;
        }
        self.saved
            .sessions
            .iter()
            .find(|session| session.expires > now() && matches(&session.hash, secret))
            .map(|session| session.id.clone())
    }
    pub fn active(&self, id: &str) -> bool {
        self.saved
            .sessions
            .iter()
            .any(|session| session.id == id && session.expires > now())
    }
    pub fn revoke(&mut self, id: &str) -> io::Result<()> {
        let mut saved = self.saved.clone();
        saved.sessions.retain(|session| session.id != id);
        self.persist(&saved)?;
        self.saved = saved;
        Ok(())
    }
    pub fn sessions(&self) -> serde_json::Value {
        serde_json::Value::Array(self.saved.sessions.iter().filter(|session| session.expires > now()).map(|session| serde_json::json!({"id":session.id,"label":session.label,"expires":session.expires})).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pairing_is_one_use_persistent_and_revocable_without_storing_secrets() {
        let folder = std::env::temp_dir().join(format!("ferrite-access-{}", random_secret(8)));
        let mut access = Access::open(&folder).unwrap();
        let identity = access.descriptor().server_id;
        assert_eq!(identity, crate::server_id::open(&folder).unwrap().as_str());
        let code = access.issue().unwrap();
        let granted = access.exchange(&code, "test desktop").unwrap();
        assert_ne!(code, granted.secret);
        assert!(access.exchange(&code, "replay").is_err());
        let bytes = std::fs::read_to_string(folder.join("remote-access.json")).unwrap();
        assert!(!bytes.contains(&code));
        assert!(!bytes.contains(&granted.secret));
        assert!(!bytes.contains(&identity), "the Server keeps its id itself");
        let mut reopened = Access::open(&folder).unwrap();
        assert_eq!(identity, reopened.descriptor().server_id);
        assert_eq!(
            reopened.authorize(&granted.secret),
            Some(granted.session_id.clone())
        );
        reopened.revoke(&granted.session_id).unwrap();
        assert!(reopened.authorize(&granted.secret).is_none());
        assert!(!Access::open(&folder).unwrap().active(&granted.session_id));
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn expired_codes_and_excessive_auth_requests_are_rejected() {
        let folder = std::env::temp_dir().join(format!("ferrite-access-{}", random_secret(8)));
        let mut access = Access::open(&folder).unwrap();
        let code = access.issue().unwrap();
        access.saved.grants[0].expires = 0;
        assert!(access.exchange(&code, "expired").is_err());
        let code = access.issue().unwrap();
        for _ in 0..20 {
            let _ = access.exchange("incorrect", "attempt");
        }
        assert!(access.exchange(&code, "limited").is_err());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
