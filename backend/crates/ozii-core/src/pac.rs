use crate::domain::DomainAllowlist;

pub struct PacGenerator {
    allowlist: DomainAllowlist,
}

impl PacGenerator {
    pub fn new(allowlist: DomainAllowlist) -> Self {
        Self { allowlist }
    }

    /// Generates a PAC file body based on the allowlist and target proxy.
    /// proxy_addr should be e.g., "127.0.0.1:8080".
    pub fn generate_pac_script(
        &self,
        proxy_addr: &str,
        original_proxy_server: Option<&str>,
    ) -> String {
        // We need to output JavaScript that evaluates the domain.
        // We will build a list of JS rules from our allowlist.

        let mut js_rules = String::new();

        let mut first = true;
        for exact in self.allowlist.get_exact_domains() {
            if !first {
                js_rules.push_str(" ||\n        ");
            }
            js_rules.push_str(&format!("host === \"{}\"", exact));
            first = false;
        }

        for suffix in self.allowlist.get_wildcard_suffixes() {
            if !first {
                js_rules.push_str(" ||\n        ");
            }
            // suffix has a leading dot, e.g. ".discord.com". In PAC we want "*.discord.com".
            js_rules.push_str(&format!("shExpMatch(host, \"*{}\")", suffix));
            first = false;
        }

        if js_rules.is_empty() {
            js_rules.push_str("false"); // Fallback if list is empty
        }

        let fallback_route = match original_proxy_server {
            Some(srv) if !srv.is_empty() => format!("PROXY {}", srv),
            _ => "DIRECT".to_string(),
        };

        format!(
            r#"function FindProxyForURL(url, host) {{
    // Localhost & internal always direct
    if (isPlainHostName(host) ||
        host === "localhost" ||
        shExpMatch(host, "127.*") ||
        shExpMatch(host, "10.*") ||
        shExpMatch(host, "192.168.*") ||
        shExpMatch(host, "172.16.*") || shExpMatch(host, "172.17.*") ||
        shExpMatch(host, "172.18.*") || shExpMatch(host, "172.19.*") ||
        shExpMatch(host, "172.2?.*") || shExpMatch(host, "172.30.*") ||
        shExpMatch(host, "172.31.*") ||
        shExpMatch(host, "*.local") ||
        shExpMatch(host, "*.localhost") ||
        shExpMatch(host, "*.internal"))
        return "DIRECT";

    // OS Connectivity Check domains -> DIRECT
    if (shExpMatch(host, "*.msftconnecttest.com") ||
        shExpMatch(host, "*.msftncsi.com") ||
        host === "dns.msn.com" ||
        host === "ipv6.msftconnecttest.com" ||
        shExpMatch(host, "*.windowsupdate.com") ||
        shExpMatch(host, "*.delivery.mp.microsoft.com"))
        return "DIRECT";

    // Dynamic Allowlist
    if ({js_rules}) {{
        return "PROXY {proxy_addr}; DIRECT";
    }}

    // Default to existing proxy or direct
    return "{fallback_route}";
}}"#,
            js_rules = js_rules,
            proxy_addr = proxy_addr,
            fallback_route = fallback_route
        )
    }

    pub fn generate_direct_pac_script() -> String {
        r#"function FindProxyForURL(url, host) {
    return "DIRECT";
}"#
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pac_generation() {
        let allowlist = DomainAllowlist::discord_default();
        let generator = PacGenerator::new(allowlist);
        let pac = generator.generate_pac_script("127.0.0.1:8080", None);

        assert!(pac.contains("PROXY 127.0.0.1:8080; DIRECT"));
        assert!(pac.contains("host === \"discord.com\""));
        assert!(pac.contains("shExpMatch(host, \"*.discord.com\")"));

        // Ensure default is DIRECT
        assert!(pac.ends_with("return \"DIRECT\";\n}"));
    }

    #[test]
    fn test_pac_generation_with_fallback() {
        let allowlist = DomainAllowlist::discord_default();
        let generator = PacGenerator::new(allowlist);
        let pac = generator.generate_pac_script("127.0.0.1:8080", Some("192.168.1.5:3128"));

        assert!(pac.contains("PROXY 127.0.0.1:8080; DIRECT"));
        // Ensure default routes to fallback proxy
        assert!(pac.ends_with("return \"PROXY 192.168.1.5:3128\";\n}"));
    }
}
