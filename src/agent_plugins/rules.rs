//! Agent Plugins Specification v1.0.0 field rules used by conversion and validation.

/// §7.2.1: `command` MUST be a single bare executable name or a plugin-relative path
/// beginning with `./`. Placeholder expansion does not apply.
pub fn is_valid_stdio_command(command: &str) -> bool {
    if command.is_empty() || command.split_whitespace().count() != 1 {
        return false;
    }
    if command.contains("${") {
        return false;
    }
    if command.starts_with("./") {
        return true;
    }
    // Bare executable name: no path separators.
    !command.contains('/') && !command.contains('\\')
}

/// §7.2.1: when present, `cwd` MUST be `./…`, `${PLUGIN_ROOT}` / `${PLUGIN_ROOT}/…`,
/// or `${PLUGIN_DATA}` / `${PLUGIN_DATA}/…`.
pub fn is_valid_stdio_cwd(cwd: &str) -> bool {
    if cwd.starts_with("./") {
        return true;
    }
    if cwd == "${PLUGIN_ROOT}" || cwd.starts_with("${PLUGIN_ROOT}/") {
        return true;
    }
    if cwd == "${PLUGIN_DATA}" || cwd.starts_with("${PLUGIN_DATA}/") {
        return true;
    }
    false
}

/// §7.2.1 remote URL rules: absolute HTTP(S), no userinfo or fragment; non-loopback
/// endpoints MUST use HTTPS.
pub fn validate_remote_url(url: &str) -> Result<(), String> {
    if url.contains("${") {
        return Err(
            "url contains a ${...} placeholder which Agent Plugins does not expand; use a literal absolute URL"
                .to_string(),
        );
    }
    if url.contains('@') || url.contains('#') {
        return Err("url must not contain user info or a fragment".to_string());
    }

    let (scheme, rest) = match url.split_once("://") {
        Some((scheme, rest)) => (scheme, rest),
        None => return Err("url must be absolute HTTP(S)".to_string()),
    };

    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err("url must be absolute HTTP(S)".to_string());
    }

    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .split('%')
        .next()
        .unwrap_or("");
    // Strip optional :port from host (IPv6 bracket form kept intact).
    let host = if host.starts_with('[') {
        host
    } else {
        host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host)
    };

    if host.is_empty() {
        return Err("url must be absolute HTTP(S)".to_string());
    }

    if scheme == "http" && !is_loopback_host(host) {
        return Err(
            "url must use HTTPS for non-loopback endpoints (HTTP is only allowed for localhost or loopback IP literals)"
                .to_string(),
        );
    }

    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if host == "::1" || host == "[::1]" {
        return true;
    }
    // IPv4 loopback 127.0.0.0/8
    if let Some((a, rest)) = host.split_once('.')
        && a == "127"
        && rest.split('.').count() == 3
        && host.split('.').all(|octet| octet.parse::<u8>().is_ok())
    {
        return true;
    }
    false
}

/// Extract the Agent Plugins version segment from a canonical `$schema` URL.
/// Returns `Some("1.0.0")` for `https://agent-plugins.org/schemas/1.0.0/...`.
pub fn schema_version(schema: &str) -> Option<&str> {
    let rest = schema.strip_prefix("https://agent-plugins.org/schemas/")?;
    let (version, _) = rest.split_once('/')?;
    Some(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bare_and_relative_commands() {
        assert!(is_valid_stdio_command("npx"));
        assert!(is_valid_stdio_command("./bin/server"));
        assert!(is_valid_stdio_command("./"));
    }

    #[test]
    fn rejects_invalid_commands() {
        assert!(!is_valid_stdio_command(""));
        assert!(!is_valid_stdio_command("node server.js"));
        assert!(!is_valid_stdio_command("${PLUGIN_ROOT}/bin/x"));
        assert!(!is_valid_stdio_command("/usr/bin/node"));
        assert!(!is_valid_stdio_command("bin/server"));
        assert!(!is_valid_stdio_command(r"C:\tools\node"));
    }

    #[test]
    fn accepts_valid_cwd_forms() {
        assert!(is_valid_stdio_cwd("./"));
        assert!(is_valid_stdio_cwd("./data"));
        assert!(is_valid_stdio_cwd("${PLUGIN_ROOT}"));
        assert!(is_valid_stdio_cwd("${PLUGIN_ROOT}/tmp"));
        assert!(is_valid_stdio_cwd("${PLUGIN_DATA}"));
        assert!(is_valid_stdio_cwd("${PLUGIN_DATA}/cache"));
    }

    #[test]
    fn rejects_invalid_cwd_forms() {
        assert!(!is_valid_stdio_cwd("data"));
        assert!(!is_valid_stdio_cwd("/tmp"));
        assert!(!is_valid_stdio_cwd("${PLUGIN_ROOT}x"));
        assert!(!is_valid_stdio_cwd("${OTHER}/x"));
    }

    #[test]
    fn remote_url_rules() {
        assert!(validate_remote_url("https://example.com/mcp").is_ok());
        assert!(validate_remote_url("http://localhost:3000/mcp").is_ok());
        assert!(validate_remote_url("http://127.0.0.1/mcp").is_ok());
        assert!(validate_remote_url("http://example.com/mcp").is_err());
        assert!(validate_remote_url("https://user:pass@example.com/mcp").is_err());
        assert!(validate_remote_url("https://example.com/mcp#frag").is_err());
        assert!(validate_remote_url("${BASE}/mcp").is_err());
    }

    #[test]
    fn schema_version_parsing() {
        assert_eq!(
            schema_version("https://agent-plugins.org/schemas/1.0.0/plugin.schema.json"),
            Some("1.0.0")
        );
        assert_eq!(
            schema_version("https://agent-plugins.org/schemas/1.1.0/mcp.schema.json"),
            Some("1.1.0")
        );
        assert_eq!(schema_version("https://example.com/schema"), None);
    }
}
