//! The Server's id (`ServerId`), made the first time a Server starts on
//! its Store and kept in `server-id` there. The remote gateway's `Access`
//! reads the same one, so a paired link and the welcome name one Server.

use ferrite_core::machine::remote::{check_identity, random_secret, write_private_new};
use ferrite_core::server::ServerId;
use std::{io, path::Path};

/// The Store's id, made and kept if it has none. Two processes opening one
/// Store at once agree: the first to keep its id wins and the other reads it.
pub fn open(store: &Path) -> io::Result<ServerId> {
    let path = store.join("server-id");
    match read(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        read => return read,
    }
    let id = remote_identity(store).unwrap_or_else(|| random_secret(16));
    match write_private_new(&path, id.as_bytes()) {
        Ok(()) => Ok(ServerId::new(id)),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => read(&path),
        Err(error) => Err(error),
    }
}

fn read(path: &Path) -> io::Result<ServerId> {
    let id = std::fs::read_to_string(path)?;
    check_identity(&id).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(ServerId::new(id))
}

/// The identity remote access kept before every Server had an id: windows
/// paired with this Store dial it by that one, so it is kept.
fn remote_identity(store: &Path) -> Option<String> {
    let bytes = std::fs::read(store.join("remote-access.json")).ok()?;
    let saved: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let id = saved.get("server_id")?.as_str()?;
    check_identity(id).is_ok().then(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ferrite-server-id-{name}-{}", random_secret(8)))
    }

    #[test]
    fn a_store_keeps_the_id_it_was_first_given() {
        let store = store("kept");
        let first = open(&store).unwrap();
        assert!(check_identity(first.as_str()).is_ok());
        assert_eq!(open(&store).unwrap(), first);
        std::fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn a_store_paired_before_ids_keeps_its_remote_identity() {
        let store = store("paired");
        std::fs::create_dir_all(&store).unwrap();
        let paired = "0123456789abcdef0123456789abcdef";
        std::fs::write(
            store.join("remote-access.json"),
            serde_json::json!({"server_id": paired, "name": "box", "grants": [], "sessions": []})
                .to_string(),
        )
        .unwrap();
        assert_eq!(open(&store).unwrap().as_str(), paired);
        std::fs::remove_dir_all(store).unwrap();
    }
}
