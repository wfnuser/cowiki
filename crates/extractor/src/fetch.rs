use crate::ExtractError;
use std::net::IpAddr;
use std::time::Duration;

const MAX_BODY_BYTES: usize = 10 * 1024 * 1024; // 10 MB
const MAX_REDIRECTS: usize = 5;
const USER_AGENT: &str =
    "Mozilla/5.0 (compatible; cowiki-extractor/0.1; +https://github.com/cowiki)";

/// True if `ip` is a globally-routable public address (i.e. NOT loopback,
/// private, link-local, CGNAT, multicast, unspecified, etc.). Used to block
/// SSRF into cloud metadata (169.254.169.254), localhost, and internal hosts.
fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local() // 169.254.0.0/16 (incl. cloud metadata)
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                // CGNAT / shared address space 100.64.0.0/10
                || (o[0] == 100 && (o[1] & 0xc0) == 0x40))
        }
        IpAddr::V6(v6) => {
            let seg0 = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (seg0 & 0xfe00) == 0xfc00  // unique-local fc00::/7
                || (seg0 & 0xffc0) == 0xfe80) // link-local fe80::/10
        }
    }
}

/// Validate that `raw` is an http(s) URL whose host resolves only to public IPs.
async fn validate_url(raw: &str) -> Result<(), ExtractError> {
    let parsed = url::Url::parse(raw)?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(ExtractError::Blocked(format!(
                "scheme '{other}' not allowed"
            )))
        }
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| ExtractError::Blocked("missing host".into()))?
        .to_string();
    let port = parsed.port_or_known_default().unwrap_or(80);

    // Resolve and check EVERY address the host maps to (defends against a domain
    // that resolves to a private IP, e.g. DNS rebinding).
    let mut saw_addr = false;
    let addrs = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|e| ExtractError::Blocked(format!("DNS resolution failed: {e}")))?;
    for addr in addrs {
        saw_addr = true;
        if !is_public_ip(addr.ip()) {
            return Err(ExtractError::Blocked(format!(
                "host resolves to non-public address {}",
                addr.ip()
            )));
        }
    }
    if !saw_addr {
        return Err(ExtractError::Blocked("host did not resolve".into()));
    }
    Ok(())
}

/// Fetch the HTML content at `url`, enforcing a 10 MB size limit and SSRF guards:
/// only http(s), host must resolve to public addresses, and redirects are followed
/// manually (re-validating each hop) up to `MAX_REDIRECTS`.
pub async fn fetch_url(url: &str) -> Result<String, ExtractError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let mut current = url.to_string();
    let mut hops = 0usize;

    loop {
        validate_url(&current).await?;
        let response = client.get(&current).send().await?;

        if response.status().is_redirection() {
            hops += 1;
            if hops > MAX_REDIRECTS {
                return Err(ExtractError::TooManyRedirects);
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| ExtractError::Blocked("redirect without Location".into()))?;
            // Resolve relative redirects against the current URL.
            let base = url::Url::parse(&current)?;
            let next = base.join(location)?;
            current = next.to_string();
            continue;
        }

        // Check Content-Length header first to avoid downloading oversized responses
        if let Some(content_length) = response.content_length() {
            if content_length as usize > MAX_BODY_BYTES {
                return Err(ExtractError::TooLarge);
            }
        }

        let bytes = response.bytes().await?;
        if bytes.len() > MAX_BODY_BYTES {
            return Err(ExtractError::TooLarge);
        }
        return Ok(String::from_utf8_lossy(&bytes).into_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_ips() {
        for ip in [
            "127.0.0.1",
            "10.0.0.5",
            "192.168.1.1",
            "172.16.0.1",
            "169.254.169.254", // cloud metadata
            "100.64.0.1",      // CGNAT
            "0.0.0.0",
            "::1",
            "fc00::1",
            "fe80::1",
        ] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip} should be blocked");
        }
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.216.34",
            "2606:4700:4700::1111",
        ] {
            assert!(is_public_ip(ip.parse().unwrap()), "{ip} should be allowed");
        }
    }

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        assert!(matches!(
            validate_url("file:///etc/passwd").await,
            Err(ExtractError::Blocked(_))
        ));
        assert!(matches!(
            validate_url("ftp://example.com/x").await,
            Err(ExtractError::Blocked(_))
        ));
    }

    #[tokio::test]
    async fn rejects_private_ip_hosts() {
        for u in [
            "http://127.0.0.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.1/",
            "http://[::1]/",
        ] {
            assert!(
                matches!(validate_url(u).await, Err(ExtractError::Blocked(_))),
                "{u} should be blocked"
            );
        }
    }
}
