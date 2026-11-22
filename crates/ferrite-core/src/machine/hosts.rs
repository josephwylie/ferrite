//! Suggestions only. OpenSSH remains responsible for resolving each destination.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::Machine;
use crate::spawn::NoConsoleWindow;

pub fn candidates() -> Vec<String> {
    discover()
        .unwrap_or_default()
        .into_iter()
        .map(|host| host.alias)
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    pub alias: String,
    pub hostname: String,
    pub username: Option<String>,
    pub port: Option<u16>,
    pub source: String,
}

pub fn discover() -> Result<Vec<Host>, String> {
    if let Some(directory) = std::env::var_os("FERRITE_SSH_HOSTS_DIR") {
        return discover_in(Path::new(&directory));
    }
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) else {
        return Ok(Vec::new());
    };
    discover_in(&Path::new(&home).join(".ssh"))
}

fn discover_in(directory: &Path) -> Result<Vec<Host>, String> {
    let mut discovery = Discovery {
        directory,
        visited: BTreeSet::new(),
        hosts: BTreeMap::new(),
        hostnames: BTreeSet::new(),
        bytes: 0,
    };
    discovery.config(&directory.join("config"))?;
    let mut known = String::new();
    if let Ok(file) = std::fs::File::open(directory.join("known_hosts")) {
        file.take(4_194_305)
            .read_to_string(&mut known)
            .map_err(|error| format!("Could not read known SSH hosts: {error}"))?;
        if known.len() > 4_194_304 {
            return Err("Known SSH hosts is larger than 4 MiB.".into());
        }
    }
    for alias in parse("", &known) {
        discovery.hosts.entry(alias.clone()).or_insert_with(|| {
            let (hostname, username, port) = Machine::new(&alias).ssh_fields();
            Host {
                alias,
                hostname,
                username: (!username.is_empty()).then_some(username),
                port: port.parse().ok(),
                source: "known_hosts".into(),
            }
        });
    }
    Ok(discovery.hosts.into_values().collect())
}

struct Discovery<'a> {
    directory: &'a Path,
    visited: BTreeSet<PathBuf>,
    hosts: BTreeMap<String, Host>,
    hostnames: BTreeSet<String>,
    bytes: usize,
}

impl Discovery<'_> {
    fn config(&mut self, path: &Path) -> Result<(), String> {
        let canonical = match path.canonicalize() {
            Ok(path) if path.is_file() => path,
            _ => return Ok(()),
        };
        if self.visited.contains(&canonical) {
            return Ok(());
        }
        if self.visited.len() >= 128 {
            return Err("SSH configuration includes too many files.".into());
        }
        self.visited.insert(canonical.clone());
        let mut text = String::new();
        std::fs::File::open(&canonical)
            .and_then(|file| file.take(1_048_577).read_to_string(&mut text))
            .map_err(|error| format!("Could not read SSH configuration: {error}"))?;
        self.bytes += text.len();
        if self.bytes > 1_048_576 {
            return Err("SSH configuration is larger than 1 MiB.".into());
        }
        let mut aliases = Vec::new();
        for line in text.lines() {
            let Some(mut words) = shlex::split(line) else {
                continue;
            };
            if words.is_empty() {
                continue;
            }
            if let Some((key, value)) = words[0].split_once('=') {
                let (key, value) = (key.to_owned(), value.to_owned());
                words[0] = key;
                words.insert(1, value);
            }
            let key = words.remove(0).to_ascii_lowercase();
            if words.first().is_some_and(|word| word == "=") {
                words.remove(0);
            }
            match key.as_str() {
                "host" => {
                    aliases = words
                        .into_iter()
                        .filter(|alias| !alias.contains(['*', '?', '!']))
                        .filter(|alias| Machine::from_ssh_fields(alias, "", "").is_ok())
                        .collect();
                    for alias in &aliases {
                        self.hosts.entry(alias.clone()).or_insert_with(|| Host {
                            alias: alias.clone(),
                            hostname: alias.clone(),
                            username: None,
                            port: None,
                            source: "config".into(),
                        });
                    }
                }
                "match" => aliases.clear(),
                "include" => {
                    for pattern in words {
                        let pattern = if let Some(rest) = pattern.strip_prefix("~/") {
                            self.directory.parent().unwrap_or(self.directory).join(rest)
                        } else if Path::new(&pattern).is_absolute() {
                            PathBuf::from(pattern)
                        } else {
                            self.directory.join(pattern)
                        };
                        let Some(pattern) = pattern.to_str() else {
                            continue;
                        };
                        if let Ok(paths) = glob::glob(pattern) {
                            for path in paths.flatten() {
                                self.config(&path)?;
                            }
                        }
                    }
                }
                "hostname" | "user" | "port" if words.len() == 1 => {
                    for alias in &aliases {
                        let Some(host) = self.hosts.get_mut(alias) else {
                            continue;
                        };
                        match key.as_str() {
                            "hostname"
                                if self.hostnames.insert(alias.clone())
                                    && !words[0].contains(['$', '%']) =>
                            {
                                host.hostname = words[0].clone()
                            }
                            "user" if host.username.is_none() && !words[0].contains(['$', '%']) => {
                                host.username = Some(words[0].clone())
                            }
                            "port" if host.port.is_none() => {
                                host.port = words[0].parse::<u16>().ok().filter(|port| *port > 0)
                            }
                            _ => (),
                        }
                    }
                }
                _ => (),
            }
        }
        Ok(())
    }
}

pub fn resolve(target: &str) -> Result<Host, String> {
    Machine::from_ssh_fields(target, "", "")?;
    let mut child = Command::new("ssh")
        .args(["-G", "--", target])
        .no_console_window()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not resolve SSH host: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("SSH did not provide its configuration.")?;
    let output = std::thread::spawn(move || {
        let mut text = String::new();
        stdout
            .take(1_048_577)
            .read_to_string(&mut text)
            .map(|_| text)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(match result {
                    Err(error) => error.to_string(),
                    _ => "SSH host resolution timed out.".into(),
                });
            }
        }
    };
    if !status.success() {
        return Err("SSH could not resolve this host. Check your SSH configuration.".into());
    }
    let text = output
        .join()
        .map_err(|_| "SSH host resolution stopped.")?
        .map_err(|error| error.to_string())?;
    if text.len() > 1_048_576 {
        return Err("SSH host configuration is too large.".into());
    }
    let mut host = Host {
        alias: target.into(),
        hostname: target.into(),
        username: None,
        port: None,
        source: "config".into(),
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once(' ') else {
            continue;
        };
        match key {
            "hostname" => host.hostname = value.into(),
            "user" => host.username = Some(value.into()),
            "port" => host.port = value.parse().ok(),
            _ => (),
        }
    }
    Ok(host)
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

    struct Config(PathBuf);

    impl Config {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("ferrite-hosts-{}-{id}", std::process::id()));
            std::fs::create_dir_all(path.join(".ssh/config.d")).unwrap();
            Self(path)
        }

        fn write(&self, path: &str, text: &str) {
            std::fs::write(self.0.join(".ssh").join(path), text).unwrap();
        }

        fn hosts(&self) -> Vec<Host> {
            discover_in(&self.0.join(".ssh")).unwrap()
        }
    }

    impl Drop for Config {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn discovers_included_hosts_with_globs_quotes_and_cycles() {
        let config = Config::new();
        config.write(
            "config",
            "Include config.d/*.conf\nHost direct\n HostName direct.example\n",
        );
        config.write("config.d/team.conf", "Include config\nHost included-devbox\n HostName example.invalid\n User builder\n Port 2222\nInclude \"config.d/space file\"\n");
        config.write(
            "config.d/space file",
            "Host quoted\n HostName quoted.example\n",
        );
        let hosts = config.hosts();
        assert_eq!(
            hosts
                .iter()
                .map(|host| host.alias.as_str())
                .collect::<Vec<_>>(),
            ["direct", "included-devbox", "quoted"]
        );
        let host = hosts
            .iter()
            .find(|host| host.alias == "included-devbox")
            .unwrap();
        assert_eq!(host.hostname, "example.invalid");
        assert_eq!(host.username.as_deref(), Some("builder"));
        assert_eq!(host.port, Some(2222));
    }

    #[test]
    fn resolves_relative_includes_from_ssh_directory_and_expands_home() {
        let config = Config::new();
        config.write(
            "config",
            "iNcLuDe=\"config.d/team.conf\"\nInclude ~/.ssh/home.conf\n",
        );
        config.write(
            "config.d/team.conf",
            "Include another.conf\nHost * !negated wildcard?\nHost team\n",
        );
        config.write("another.conf", "Host nested\n");
        config.write("home.conf", "Host home\n");
        assert_eq!(
            config
                .hosts()
                .iter()
                .map(|host| host.alias.as_str())
                .collect::<Vec<_>>(),
            ["home", "nested", "team"]
        );
    }

    #[test]
    fn discovery_preserves_known_host_ports_and_first_config_values() {
        let config = Config::new();
        config.write("config", "Host box\n HostName first.example\n User first\n Port 2222\nHost box\n HostName later.example\n User later\n Port 4444\n");
        config.write(
            "known_hosts",
            "box ssh-ed25519 fixture\n[other]:2200 ssh-ed25519 fixture\n",
        );
        let hosts = config.hosts();
        assert_eq!(hosts[0].hostname, "first.example");
        assert_eq!(hosts[0].username.as_deref(), Some("first"));
        assert_eq!(hosts[0].port, Some(2222));
        assert_eq!(hosts[1].alias, "ssh://other:2200");
        assert_eq!(hosts[1].port, Some(2200));
    }

    #[test]
    fn discovery_bounds_configuration_reads() {
        let config = Config::new();
        config.write("config", &"#".repeat(1_048_577));
        assert!(discover_in(&config.0.join(".ssh"))
            .unwrap_err()
            .contains("1 MiB"));
    }

    #[test]
    fn resolution_rejects_options_before_starting_ssh() {
        assert!(resolve("-oProxyCommand=bad").is_err());
    }

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
