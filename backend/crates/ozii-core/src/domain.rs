use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct DomainAllowlist {
    /// Exact match domains (e.g., discord.com)
    exact_domains: HashSet<String>,
    /// Wildcard suffixes (e.g., .discord.com) - Note the leading dot for boundary matching
    wildcard_suffixes: Vec<String>,
}

impl Default for DomainAllowlist {
    fn default() -> Self {
        Self::ozii_default()
    }
}

impl DomainAllowlist {
    pub fn new() -> Self {
        Self {
            exact_domains: HashSet::new(),
            wildcard_suffixes: Vec::new(),
        }
    }

    pub fn discord_default() -> Self {
        let mut list = Self::new();
        list.add_domain("discord.com");
        list.add_domain("discord.gg");
        list.add_domain("discordapp.com");
        list.add_domain("discordapp.net");
        list.add_domain("discord.media");
        list.add_domain("discord.app");
        list.add_domain("discord.dev");
        list.add_domain("discord.new");
        list.add_domain("discord.gift");
        list.add_domain("discord.co");
        list.add_domain("discordstatus.com");
        list.add_domain("dis.gd");
        list
    }

    /// Default allowlist used by the running app: Discord + YouTube
    /// (YouTube is DPI-throttled on many ISPs and needs engine bypass too).
    pub fn ozii_default() -> Self {
        let mut list = Self::discord_default();
        list.add_domain("youtube.com");
        list.add_domain("youtu.be");
        list.add_domain("youtube-nocookie.com");
        list.add_domain("googlevideo.com");
        list.add_domain("ytimg.com");
        list.add_domain("ggpht.com");
        list
    }

    /// Adds a domain to the allowlist. Automatically handles wildcard semantics.
    /// If you add "discord.com", it matches "discord.com" and "*.discord.com".
    pub fn add_domain(&mut self, domain: &str) {
        let domain = domain.to_lowercase();
        let domain = domain.strip_prefix("*.").unwrap_or(&domain);
        let domain = domain.strip_suffix('.').unwrap_or(domain);

        self.exact_domains.insert(domain.to_string());

        let suffix = format!(".{}", domain);
        if !self.wildcard_suffixes.contains(&suffix) {
            self.wildcard_suffixes.push(suffix);
        }
    }

    /// Checks if a given hostname matches the allowlist.
    /// E.g. "discord.com" -> true, "api.discord.com" -> true, "evil-discord.com" -> false
    pub fn is_allowed(&self, hostname: &str) -> bool {
        let hostname = hostname.to_lowercase();
        let hostname = hostname.strip_suffix('.').unwrap_or(&hostname);

        if self.exact_domains.contains(hostname) {
            return true;
        }

        for suffix in &self.wildcard_suffixes {
            if hostname.ends_with(suffix) {
                return true;
            }
        }

        false
    }

    pub fn get_exact_domains(&self) -> impl Iterator<Item = &String> {
        self.exact_domains.iter()
    }

    pub fn get_wildcard_suffixes(&self) -> impl Iterator<Item = &String> {
        self.wildcard_suffixes.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discord_default_routing() {
        let allowlist = DomainAllowlist::discord_default();

        // Exact matches
        assert!(allowlist.is_allowed("discord.com"));
        assert!(allowlist.is_allowed("discord.gg"));
        assert!(allowlist.is_allowed("discordapp.com"));
        assert!(allowlist.is_allowed("discordapp.net"));

        // Subdomain matches
        assert!(allowlist.is_allowed("api.discord.com"));
        assert!(allowlist.is_allowed("gateway.discord.gg"));
        assert!(allowlist.is_allowed("media.discordapp.net"));
        assert!(allowlist.is_allowed("cdn.discordapp.com"));

        // Deep subdomain matches
        assert!(allowlist.is_allowed("deep.sub.discord.com"));

        // Should NOT match prefix/suffix spoofing
        assert!(!allowlist.is_allowed("evil-discord.com"));
        assert!(!allowlist.is_allowed("discord.com.example.com"));
        assert!(!allowlist.is_allowed("discord.com.tr"));
        assert!(!allowlist.is_allowed("mydiscordapp.com"));

        // Completely unrelated
        assert!(!allowlist.is_allowed("google.com"));
        assert!(!allowlist.is_allowed("steam.com"));
    }

    #[test]
    fn test_ozii_default_includes_youtube() {
        let allowlist = DomainAllowlist::ozii_default();

        assert!(allowlist.is_allowed("youtube.com"));
        assert!(allowlist.is_allowed("www.youtube.com"));
        assert!(allowlist.is_allowed("music.youtube.com"));
        assert!(allowlist.is_allowed("youtu.be"));
        assert!(allowlist.is_allowed("www.youtube-nocookie.com"));
        assert!(allowlist.is_allowed("rr5---sn-xyz.googlevideo.com"));
        assert!(allowlist.is_allowed("yt3.ggpht.com"));
        assert!(allowlist.is_allowed("i.ytimg.com"));
        assert!(!allowlist.is_allowed("evil-youtube.com"));
        assert!(!allowlist.is_allowed("youtube.com.example.com"));
        // Discord still covered
        assert!(allowlist.is_allowed("discord.com"));
    }

    #[test]
    fn test_domain_validation_cases() {
        let allowlist = DomainAllowlist::ozii_default();
        for bad in ["youtube.com..", ".mydomain", "https://youtube.com"] {
            assert!(
                !allowlist.is_allowed(&bad.to_lowercase()),
                "unexpected match: {bad}"
            );
        }
    }
}
