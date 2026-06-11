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
                || (o[0] == 100 && (o[1] & 0xc0) == 0x40)
                // benchmarking 198.18.0.0/15
                || (o[0] == 198 && (o[1] & 0xfe) == 18)
                // IETF protocol assignments 192.0.0.0/24
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                // reserved 240.0.0.0/4
                || o[0] >= 240)
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped (::ffff:a.b.c.d) and NAT64 (64:ff9b::/96) addresses embed an
            // IPv4 target the OS will actually route to — classify by the embedded v4,
            // otherwise ::ffff:127.0.0.1 / ::ffff:169.254.169.254 sail through as
            // "public" v6 and reach loopback/metadata.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let segs = v6.segments();
            if segs[0] == 0x0064 && segs[1] == 0xff9b && segs[2..6] == [0, 0, 0, 0] {
                let v4 = std::net::Ipv4Addr::new(
                    (segs[6] >> 8) as u8,
                    (segs[6] & 0xff) as u8,
                    (segs[7] >> 8) as u8,
                    (segs[7] & 0xff) as u8,
                );
                return is_public_ip(IpAddr::V4(v4));
            }
            let seg0 = segs[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (seg0 & 0xfe00) == 0xfc00  // unique-local fc00::/7
                || (seg0 & 0xffc0) == 0xfe80) // link-local fe80::/10
        }
    }
}

/// Validate that `raw` is an http(s) URL whose host resolves only to public IPs.
/// Returns the validated `(host, first resolved address)` so the caller can **pin**
/// the connection to exactly the address that was checked — without pinning, reqwest
/// re-resolves the host and a rebinding DNS server (low TTL) can answer public here
/// and private on the second lookup (TOCTOU).
async fn validate_url(raw: &str) -> Result<(String, std::net::SocketAddr), ExtractError> {
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
    // whose record set mixes public and private addresses).
    let mut first: Option<std::net::SocketAddr> = None;
    let addrs = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|e| ExtractError::Blocked(format!("DNS resolution failed: {e}")))?;
    for addr in addrs {
        if !is_public_ip(addr.ip()) {
            return Err(ExtractError::Blocked(format!(
                "host resolves to non-public address {}",
                addr.ip()
            )));
        }
        first.get_or_insert(addr);
    }
    match first {
        Some(addr) => Ok((host, addr)),
        None => Err(ExtractError::Blocked("host did not resolve".into())),
    }
}

/// Fetch the HTML content at `url`, enforcing a 10 MB size limit and SSRF guards:
/// only http(s); the host must resolve to public addresses and the connection is
/// **pinned to the validated IP**; redirects are followed manually (re-validating
/// and re-pinning each hop) up to `MAX_REDIRECTS`.
pub async fn fetch_url(url: &str) -> Result<String, ExtractError> {
    let mut current = url.to_string();
    let mut hops = 0usize;

    loop {
        let (host, pinned) = validate_url(&current).await?;
        // Per-hop client so `resolve()` pins this hop's host to the address we just
        // vetted; reqwest then connects there instead of resolving again.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .redirect(reqwest::redirect::Policy::none())
            .resolve(&host, pinned)
            .build()?;
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
            // IPv4-mapped IPv6 must classify by the embedded v4 target
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "::ffff:10.0.0.1",
            // NAT64 well-known prefix embedding loopback
            "64:ff9b::7f00:1",
            // reserved / benchmarking v4 ranges
            "240.0.0.1",
            "198.18.0.1",
            "192.0.0.1",
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
