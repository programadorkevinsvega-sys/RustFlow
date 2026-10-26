//! Trusted-proxy and forwarded-header policy for the HTTP adapter.
//!
//! Forwarded client addresses and schemes are reconstructed only when the TCP
//! peer is in the configured trusted-proxy set. Spoofed `Forwarded`,
//! `X-Forwarded-For`, and `X-Forwarded-Proto` headers from untrusted peers are
//! ignored.

use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    str::FromStr,
};

use http::HeaderMap;

use crate::ServerError;

/// How the adapter reconstructs the client address and HTTPS scheme.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ClientAddressPolicy {
    /// Use the TCP peer only. Ignore forwarded headers.
    #[default]
    Direct,
    /// Honor forwarded headers only when the TCP peer matches one of these networks.
    TrustedProxies(Vec<IpNetwork>),
}

/// An IPv4 or IPv6 CIDR network used to describe trusted reverse-proxy peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpNetwork {
    addr: IpAddr,
    prefix_len: u8,
}

impl IpNetwork {
    /// Parse `127.0.0.1`, `10.0.0.0/8`, `::1`, or `2001:db8::/32`.
    pub fn parse(value: &str) -> Result<Self, ServerError> {
        value.parse()
    }

    /// Return true when `ip` is inside this network.
    pub fn contains(self, ip: IpAddr) -> bool {
        match (self.addr, ip) {
            (IpAddr::V4(network), IpAddr::V4(candidate)) => {
                ipv4_prefix_contains(network, self.prefix_len, candidate)
            }
            (IpAddr::V6(network), IpAddr::V6(candidate)) => {
                ipv6_prefix_contains(network, self.prefix_len, candidate)
            }
            _ => false,
        }
    }
}

impl FromStr for IpNetwork {
    type Err = ServerError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (addr_text, prefix_text) = match value.split_once('/') {
            Some((addr, prefix)) => (addr, Some(prefix)),
            None => (value, None),
        };
        let addr = addr_text
            .parse::<IpAddr>()
            .map_err(|_| ServerError::InvalidIpNetwork(value.to_owned()))?;
        let max_prefix = if addr.is_ipv4() { 32 } else { 128 };
        let prefix_len = match prefix_text {
            Some(prefix) => prefix
                .parse::<u8>()
                .ok()
                .filter(|len| *len <= max_prefix)
                .ok_or_else(|| ServerError::InvalidIpNetwork(value.to_owned()))?,
            None => max_prefix,
        };
        Ok(Self { addr, prefix_len })
    }
}

impl fmt::Display for IpNetwork {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.addr, self.prefix_len)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClientIdentity {
    pub client_ip: Option<IpAddr>,
    pub https: bool,
}

pub(crate) fn resolve_client_identity(
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
    policy: &ClientAddressPolicy,
    connection_secure: bool,
) -> ClientIdentity {
    let peer_ip = peer.map(|address| address.ip());
    match policy {
        ClientAddressPolicy::Direct => ClientIdentity {
            client_ip: peer_ip,
            https: connection_secure,
        },
        ClientAddressPolicy::TrustedProxies(proxies) => {
            let Some(peer_ip) = peer_ip else {
                return ClientIdentity {
                    client_ip: None,
                    https: connection_secure,
                };
            };
            if !proxies.iter().any(|network| network.contains(peer_ip)) {
                return ClientIdentity {
                    client_ip: Some(peer_ip),
                    https: connection_secure,
                };
            }
            let forwarded = parse_forwarded_chain(headers);
            let client_ip = client_ip_from_trusted_chain(peer_ip, &forwarded.for_ips, proxies);
            let https = connection_secure
                || forwarded
                    .proto
                    .as_deref()
                    .is_some_and(|proto| proto.eq_ignore_ascii_case("https"));
            ClientIdentity {
                client_ip: Some(client_ip),
                https,
            }
        }
    }
}

struct ForwardedChain {
    for_ips: Vec<IpAddr>,
    proto: Option<String>,
}

fn parse_forwarded_chain(headers: &HeaderMap) -> ForwardedChain {
    if let Some(forwarded) = header_values(headers, "forwarded") {
        return parse_rfc7239_forwarded(&forwarded);
    }
    ForwardedChain {
        for_ips: header_values(headers, "x-forwarded-for")
            .as_deref()
            .map(parse_x_forwarded_for)
            .unwrap_or_default(),
        proto: header_values(headers, "x-forwarded-proto")
            .as_deref()
            .and_then(parse_forwarded_proto),
    }
}

fn header_values(headers: &HeaderMap, name: &'static str) -> Option<String> {
    let values = headers.get_all(name);
    let mut joined = String::new();
    for value in values {
        let Ok(text) = value.to_str() else {
            continue;
        };
        if !joined.is_empty() {
            joined.push(',');
        }
        joined.push_str(text);
    }
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

fn parse_rfc7239_forwarded(value: &str) -> ForwardedChain {
    let mut for_ips = Vec::new();
    let mut proto = None;
    for element in value.split(',') {
        // Each proxy adds the next element. Only the rightmost element represents
        // the immediately preceding hop to the trusted proxy at our boundary.
        // Do not fall back to a proto in an earlier element: it may be client-supplied.
        let mut element_proto = None;
        let mut seen_proto = false;
        let mut duplicate_proto = false;
        for pair in element.split(';') {
            let Some((name, raw_value)) = pair.trim().split_once('=') else {
                continue;
            };
            let name = name.trim();
            if name.eq_ignore_ascii_case("for") {
                if let Some(ip) = parse_forwarded_for_token(raw_value) {
                    for_ips.push(ip);
                }
            } else if name.eq_ignore_ascii_case("proto") {
                if seen_proto {
                    duplicate_proto = true;
                } else {
                    seen_proto = true;
                    element_proto = parse_forwarded_proto(raw_value);
                }
            }
        }
        // RFC 7239 parameter names are case-insensitive. An absent, invalid,
        // or duplicated proto on the rightmost element must fail closed.
        proto = if duplicate_proto { None } else { element_proto };
    }
    ForwardedChain { for_ips, proto }
}

fn parse_x_forwarded_for(value: &str) -> Vec<IpAddr> {
    value
        .split(',')
        .filter_map(|token| parse_ip_token(token.trim()))
        .collect()
}

fn parse_forwarded_proto(value: &str) -> Option<String> {
    let token = value
        .split(',')
        .next()
        .unwrap_or(value)
        .trim()
        .trim_matches('"');
    if token.eq_ignore_ascii_case("http") || token.eq_ignore_ascii_case("https") {
        Some(token.to_ascii_lowercase())
    } else {
        None
    }
}

fn parse_forwarded_for_token(token: &str) -> Option<IpAddr> {
    parse_ip_token(token.trim().trim_matches('"'))
}

fn parse_ip_token(token: &str) -> Option<IpAddr> {
    let token = token.trim().trim_matches('"');
    if token.eq_ignore_ascii_case("unknown") {
        return None;
    }
    if let Some(bracketed) = token.strip_prefix('[') {
        let host = bracketed.split(']').next().unwrap_or(bracketed);
        return host.parse().ok();
    }
    if let Ok(ip) = token.parse() {
        return Some(ip);
    }
    token.rsplit_once(':').and_then(|(host, port)| {
        port.chars()
            .all(|character| character.is_ascii_digit())
            .then(|| host.parse().ok())
            .flatten()
    })
}

fn client_ip_from_trusted_chain(
    peer: IpAddr,
    forwarded_for: &[IpAddr],
    trusted: &[IpNetwork],
) -> IpAddr {
    let mut chain = forwarded_for.to_vec();
    chain.push(peer);
    while chain.len() > 1 {
        let Some(rightmost) = chain.last().copied() else {
            break;
        };
        if trusted.iter().any(|network| network.contains(rightmost)) {
            chain.pop();
        } else {
            break;
        }
    }
    chain.last().copied().unwrap_or(peer)
}

fn ipv4_prefix_contains(network: Ipv4Addr, prefix_len: u8, candidate: Ipv4Addr) -> bool {
    if prefix_len == 0 {
        return true;
    }
    let shift = 32 - u32::from(prefix_len);
    let mask = u32::MAX << shift;
    (u32::from(network) & mask) == (u32::from(candidate) & mask)
}

fn ipv6_prefix_contains(network: Ipv6Addr, prefix_len: u8, candidate: Ipv6Addr) -> bool {
    if prefix_len == 0 {
        return true;
    }
    let shift = 128 - u32::from(prefix_len);
    let mask = u128::MAX << shift;
    (u128::from(network) & mask) == (u128::from(candidate) & mask)
}

#[cfg(test)]
mod tests {
    use super::{ClientAddressPolicy, IpNetwork, parse_ip_token, resolve_client_identity};
    use http::{HeaderMap, HeaderValue};
    use std::net::{IpAddr, SocketAddr};

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.append(
                http::HeaderName::from_bytes(name.as_bytes()).expect("header name"),
                HeaderValue::from_str(value).expect("header value"),
            );
        }
        headers
    }

    #[test]
    fn untrusted_peers_cannot_spoof_forwarded_headers() {
        let policy = ClientAddressPolicy::TrustedProxies(vec![
            IpNetwork::parse("10.0.0.0/8").expect("network"),
        ]);
        let identity = resolve_client_identity(
            Some("127.0.0.1:4444".parse::<SocketAddr>().expect("peer")),
            &headers(&[
                ("x-forwarded-for", "203.0.113.9"),
                ("x-forwarded-proto", "https"),
            ]),
            &policy,
            false,
        );
        assert_eq!(identity.client_ip, "127.0.0.1".parse::<IpAddr>().ok());
        assert!(!identity.https);
    }

    #[test]
    fn trusted_peers_expose_rightmost_untrusted_client() {
        let policy = ClientAddressPolicy::TrustedProxies(vec![
            IpNetwork::parse("127.0.0.1/32").expect("network"),
        ]);
        let identity = resolve_client_identity(
            Some("127.0.0.1:80".parse().expect("peer")),
            &headers(&[
                ("x-forwarded-for", "198.51.100.4, 127.0.0.1"),
                ("x-forwarded-proto", "https"),
            ]),
            &policy,
            false,
        );
        assert_eq!(identity.client_ip, "198.51.100.4".parse::<IpAddr>().ok());
        assert!(identity.https);
    }

    #[test]
    fn rfc7239_forwarded_is_preferred() {
        let policy =
            ClientAddressPolicy::TrustedProxies(vec![IpNetwork::parse("127.0.0.1").expect("host")]);
        let identity = resolve_client_identity(
            Some("127.0.0.1:443".parse().expect("peer")),
            &headers(&[
                ("forwarded", r#"for=203.0.113.10;proto=https"#),
                ("x-forwarded-for", "192.0.2.1"),
                ("x-forwarded-proto", "http"),
            ]),
            &policy,
            false,
        );
        assert_eq!(identity.client_ip, "203.0.113.10".parse::<IpAddr>().ok());
        assert!(identity.https);
    }

    #[test]
    fn rfc7239_uses_proto_from_rightmost_element_not_client_supplied_prefix() {
        let policy = ClientAddressPolicy::TrustedProxies(vec![
            IpNetwork::parse("127.0.0.1/32").expect("loopback"),
        ]);
        let identity = resolve_client_identity(
            Some("127.0.0.1:80".parse().expect("peer")),
            &headers(&[(
                "forwarded",
                "for=203.0.113.10;proto=https, for=198.51.100.4;proto=http",
            )]),
            &policy,
            false,
        );
        assert!(!identity.https);
    }

    #[test]
    fn rfc7239_does_not_fall_back_to_proto_from_an_earlier_element() {
        let policy = ClientAddressPolicy::TrustedProxies(vec![
            IpNetwork::parse("127.0.0.1/32").expect("loopback"),
        ]);
        let identity = resolve_client_identity(
            Some("127.0.0.1:80".parse().expect("peer")),
            &headers(&[(
                "forwarded",
                "for=203.0.113.10;proto=https, for=198.51.100.4",
            )]),
            &policy,
            false,
        );
        assert!(!identity.https);
    }

    #[test]
    fn rfc7239_parameter_names_are_case_insensitive() {
        let policy = ClientAddressPolicy::TrustedProxies(vec![
            IpNetwork::parse("127.0.0.1/32").expect("loopback"),
        ]);
        let identity = resolve_client_identity(
            Some("127.0.0.1:80".parse().expect("peer")),
            &headers(&[("forwarded", "For=203.0.113.10;Proto=https")]),
            &policy,
            false,
        );
        assert!(identity.https);
    }

    #[test]
    fn direct_policy_ignores_headers() {
        let identity = resolve_client_identity(
            Some("[::1]:9".parse().expect("peer")),
            &headers(&[("x-forwarded-proto", "https")]),
            &ClientAddressPolicy::Direct,
            false,
        );
        assert_eq!(identity.client_ip, "::1".parse::<IpAddr>().ok());
        assert!(!identity.https);
        assert!(parse_ip_token("unknown").is_none());
    }
}
