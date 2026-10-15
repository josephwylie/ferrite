//! Ferrite's stand-in for upstream's `config` (reviewr's flags and plugin config file, dropped):
//! the forge hosts a PR read resolves remotes against, and the host grammar git checks. Ferrite
//! reads the self-hosted hosts from its own settings (`forge_hosts`) instead of `config.toml`.

/// The settings a PR read takes: the self-hosted forge hosts, besides the built-in ones.
// Upstream's field names, which share its other settings' struct.
#[allow(clippy::struct_field_names)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginConfig {
    github_host: Option<String>,
    gitlab_host: Option<String>,
    azure_devops_host: Option<String>,
}

impl PluginConfig {
    /// The self-hosted hosts, each checked as upstream checks its `*_host` keys; a refused one
    /// says which key and why.
    pub fn with_hosts(
        github: Option<&str>,
        gitlab: Option<&str>,
        azure_devops: Option<&str>,
    ) -> Result<Self, String> {
        let parse =
            |key, host: Option<&str>| host.map(|host| parse_forge_host(key, host)).transpose();
        Ok(Self {
            github_host: parse("github_host", github)?,
            gitlab_host: parse("gitlab_host", gitlab)?,
            azure_devops_host: parse("azure_devops_host", azure_devops)?,
        })
    }

    pub fn github_host(&self) -> Option<&str> {
        self.github_host.as_deref()
    }

    pub fn gitlab_host(&self) -> Option<&str> {
        self.gitlab_host.as_deref()
    }

    pub fn azure_devops_host(&self) -> Option<&str> {
        self.azure_devops_host.as_deref()
    }

    /// The forge host set one fetch resolves remotes against.
    pub fn forge_hosts(&self) -> crate::git::ForgeHosts<'_> {
        crate::git::ForgeHosts {
            github: self.github_host(),
            gitlab: self.gitlab_host(),
            azure_devops: self.azure_devops_host(),
        }
    }
}

/// Parse a self-hosted forge key: a bare hostname that no built-in forge already claims.
fn parse_forge_host(key: &str, host: &str) -> Result<String, String> {
    let lower = host.to_ascii_lowercase();
    let built_in = crate::git::forge_for_host(&lower, &crate::git::ForgeHosts::default());
    if !valid_host_syntax(host) || built_in.is_some() {
        return Err(format!("{key}: expected a bare hostname outside the built-in forge hosts"));
    }
    Ok(lower)
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

#[cfg(test)]
mod tests {
    use super::PluginConfig;

    #[test]
    fn a_self_hosted_forge_is_a_bare_host_no_built_in_forge_claims() {
        let config = PluginConfig::with_hosts(Some("GHE.corp.example"), None, None).unwrap();
        assert_eq!(config.github_host(), Some("ghe.corp.example"));
        assert_eq!(config.forge_hosts().github, Some("ghe.corp.example"));
        assert!(PluginConfig::with_hosts(Some("github.com"), None, None).is_err());
        assert!(PluginConfig::with_hosts(None, Some("https://git.corp"), None).is_err());
        assert!(PluginConfig::with_hosts(None, None, Some("dev..azure")).is_err());
    }
}
