//! Ferrite's stand-in for upstream's `config` (reviewr's flags and plugin config file, dropped):
//! the forge hosts a PR read resolves remotes against, and the host grammar git checks.

/// The settings a PR read takes. Ferrite configures no self-hosted forge, so only the
/// built-in hosts resolve.
#[derive(Clone, Debug, Default)]
pub struct PluginConfig {
    _hosts: (),
}

impl PluginConfig {
    /// The forge host set one fetch resolves remotes against.
    pub fn forge_hosts(&self) -> crate::git::ForgeHosts<'_> {
        crate::git::ForgeHosts::default()
    }
}

/// The bare ASCII DNS-name grammar shared by configured and selected canonical hosts.
pub(crate) fn valid_host_syntax(host: &str) -> bool {
    if host.len() > 253 {
        return false;
    }
    let mut labels = host.split('.').peekable();
    if labels.peek().is_none() {
        return false;
    }
    labels.all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}
