//! Suggestions only. OpenSSH remains responsible for resolving each destination.

use std::collections::BTreeSet;
use std::path::Path;

use super::Machine;

pub fn candidates() -> Vec<String> {
    if let Some(directory) = std::env::var_os("FERRITE_SSH_HOSTS_DIR") {
        return read(Path::new(&directory));
    }
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) else {
        return Vec::new();
    };
    read(&Path::new(&home).join(".ssh"))
}

fn read(directory: &Path) -> Vec<String> {
    let config = std::fs::read_to_string(directory.join("config")).unwrap_or_default();
    let known = std::fs::read_to_string(directory.join("known_hosts")).unwrap_or_default();
    parse(&config, &known)
}

fn parse(config: &str, known_hosts: &str) -> Vec<String> {
    let mut hosts = BTreeSet::new();
    for line in config.lines() {
        let mut words = line
            .split('#')
            .next()
            .unwrap_or_default()
            .split_whitespace();
        if words
            .next()
            .is_some_and(|keyword| keyword.eq_ignore_ascii_case("Host"))
        {
            for host in words {
                if let Ok(machine) = Machine::from_ssh_fields(host, "", "") {
                    hosts.insert(machine.target);
                }
            }
        }
    }
    for line in known_hosts.lines() {
        let mut words = line.split_whitespace();
        let Some(mut names) = words.next() else {
            continue;
        };
        if names.starts_with('@') {
            let Some(next) = words.next() else { continue };
            names = next;
        }
        for host in names.split(',') {
            let host = if host.starts_with('[') {
                host.split_once(']').map(|(name, port)| {
                    if name[1..].parse::<std::net::Ipv6Addr>().is_ok() {
                        host.to_string()
                    } else {
                        format!("{}{}", &name[1..], port)
                    }
                })
            } else {
                Some(host.to_string())
            };
            if let Some(host) = host {
                if let Ok(machine) = Machine::from_ssh_fields(&host, "", "") {
                    hosts.insert(machine.target);
                }
            }
        }
    }
    hosts.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_literal_aliases_and_unhashed_known_hosts_without_resolving_them() {
        let hosts = parse(
            "Host cloud desktop\n HostName actual.example.com\nHost * !skip x?\nhost Cloud # comment\n",
            "box,alias ssh-ed25519 key\n|1|hashed|key ssh-ed25519 key\n[box]:2222 ssh-ed25519 key\n@cert-authority certified.example ssh-ed25519 key\n",
        );
        assert_eq!(
            hosts,
            [
                "Cloud",
                "alias",
                "box",
                "certified.example",
                "cloud",
                "desktop",
                "ssh://box:2222"
            ]
        );
    }
}
