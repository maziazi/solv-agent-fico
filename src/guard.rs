//! Guards for a publicly reachable deploy (docs/SPEC.md §8.2).
//!
//! `POST /api/v1/pre-spend-check` spends Cloudflare and SERV quota on every
//! call, and `POST /api/v1/audit-counterparty` can spend non-renewable Reppo
//! credits. So the first is rate limited per client and capped in
//! concurrency, and the second needs an admin bearer token and is closed
//! when none is configured. Every other route stays open.
//!
//! Neither guard is part of the decision: they run before the handler and
//! never change what a decision says.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::AppState;

pub const DEFAULT_RATE_LIMIT_PER_MIN: u32 = 20;
pub const DEFAULT_MAX_CONCURRENT_CHECKS: usize = 8;
/// Upper bound for `SOLV_RATE_LIMIT_PER_MIN`; it also bounds the memory one
/// client can occupy in the limiter.
pub const MAX_RATE_LIMIT_PER_MIN: u32 = 10_000;
/// Upper bound for `SOLV_MAX_CONCURRENT_CHECKS`.
pub const MAX_CONCURRENT_CHECKS: usize = 1_000;
/// Shortest admin token `main` accepts; a guessable token is no guard.
pub const MIN_ADMIN_TOKEN_CHARS: usize = 16;

const WINDOW: Duration = Duration::from_secs(60);
/// Distinct clients tracked at once. A full table that cannot be purged
/// refuses new clients rather than growing: the quota is what is being
/// protected, and the concurrency cap still bounds total spend.
const MAX_TRACKED_CLIENTS: usize = 10_000;
/// How long a caller is told to wait when the concurrency cap is full.
const CONCURRENCY_RETRY_SECS: u64 = 2;

/// Operator settings for the public gate, read once at boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitsConfig {
    /// Requests one client may make in any 60-second window.
    pub per_minute: u32,
    /// Checks that may be in flight at once, across all clients.
    pub max_concurrent: usize,
    /// Read the client address from `X-Forwarded-For` (rightmost entry).
    /// Only correct behind exactly one proxy that appends the address it
    /// saw; anywhere else the header is attacker-controlled.
    pub trust_proxy: bool,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            per_minute: DEFAULT_RATE_LIMIT_PER_MIN,
            max_concurrent: DEFAULT_MAX_CONCURRENT_CHECKS,
            trust_proxy: false,
        }
    }
}

impl LimitsConfig {
    /// Parses `SOLV_RATE_LIMIT_PER_MIN`, `SOLV_MAX_CONCURRENT_CHECKS` and
    /// `SOLV_TRUST_PROXY`. Unset or empty means the default; anything
    /// invalid is an error so a typo never silently loosens a guard.
    pub fn from_vars(
        per_minute: Option<&str>,
        max_concurrent: Option<&str>,
        trust_proxy: Option<&str>,
    ) -> Result<Self, String> {
        let defaults = Self::default();
        let per_minute = match per_minute.map(str::trim) {
            None | Some("") => defaults.per_minute,
            Some(raw) => match raw.parse::<u32>() {
                Ok(value) if (1..=MAX_RATE_LIMIT_PER_MIN).contains(&value) => value,
                _ => {
                    return Err(format!(
                        "SOLV_RATE_LIMIT_PER_MIN must be 1..={MAX_RATE_LIMIT_PER_MIN}, got {raw:?}"
                    ));
                }
            },
        };
        let max_concurrent = match max_concurrent.map(str::trim) {
            None | Some("") => defaults.max_concurrent,
            Some(raw) => match raw.parse::<usize>() {
                Ok(value) if (1..=MAX_CONCURRENT_CHECKS).contains(&value) => value,
                _ => {
                    return Err(format!(
                        "SOLV_MAX_CONCURRENT_CHECKS must be 1..={MAX_CONCURRENT_CHECKS}, got {raw:?}"
                    ));
                }
            },
        };
        let trust_proxy = match trust_proxy.map(str::trim) {
            None | Some("") | Some("0") => false,
            Some("1") => true,
            Some(raw) => {
                return Err(format!("SOLV_TRUST_PROXY must be 0 or 1, got {raw:?}"));
            }
        };
        Ok(Self {
            per_minute,
            max_concurrent,
            trust_proxy,
        })
    }
}

/// Sliding-window rate limit per client plus a global concurrency cap.
pub struct Limiter {
    config: LimitsConfig,
    permits: Arc<Semaphore>,
    seen: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
}

impl Limiter {
    pub fn new(config: LimitsConfig) -> Self {
        Self {
            config,
            permits: Arc::new(Semaphore::new(config.max_concurrent)),
            seen: Mutex::new(HashMap::new()),
        }
    }

    pub fn config(&self) -> &LimitsConfig {
        &self.config
    }

    /// `None` when the concurrency cap is full.
    pub fn try_permit(&self) -> Option<OwnedSemaphorePermit> {
        Arc::clone(&self.permits).try_acquire_owned().ok()
    }

    /// Records a request from `client` at `now`, or says how many seconds
    /// until it would be allowed. A refused request is not recorded, so
    /// hammering a refused endpoint does not extend the wait.
    pub fn check(&self, client: IpAddr, now: Instant) -> Result<(), u64> {
        let key = bucket_key(client);
        let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        if !seen.contains_key(&key) && seen.len() >= MAX_TRACKED_CLIENTS {
            seen.retain(|_, hits| {
                hits.back()
                    .is_some_and(|last| now.saturating_duration_since(*last) < WINDOW)
            });
            if seen.len() >= MAX_TRACKED_CLIENTS {
                return Err(WINDOW.as_secs());
            }
        }
        let hits = seen.entry(key).or_default();
        while hits
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= WINDOW)
        {
            hits.pop_front();
        }
        if hits.len() >= self.config.per_minute as usize {
            let waited = hits
                .front()
                .map_or(Duration::ZERO, |t| now.saturating_duration_since(*t));
            let remaining = WINDOW.saturating_sub(waited);
            return Err(remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0));
        }
        hits.push_back(now);
        Ok(())
    }
}

/// One IPv4 address is one client; one IPv6 /64 is one client, because a
/// single host is routinely handed a whole /64 and would otherwise rotate
/// through it to dodge the limit.
fn bucket_key(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => {
            let s = v6.segments();
            IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
        }
        v4 => v4,
    }
}

/// The address a request is limited under. The socket peer is the default;
/// the forwarded header is consulted only when the operator declared a
/// trusted proxy, and only its rightmost entry — the one that proxy
/// appended — because every entry to its left can be written by the caller.
pub fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>, trust_proxy: bool) -> IpAddr {
    if trust_proxy {
        if let Some(forwarded) = forwarded_client(headers) {
            return forwarded;
        }
    }
    peer.map_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED), |peer| peer.ip())
}

fn forwarded_client(headers: &HeaderMap) -> Option<IpAddr> {
    let value = headers
        .get_all("x-forwarded-for")
        .iter()
        .next_back()?
        .to_str()
        .ok()?;
    value.rsplit(',').next()?.trim().parse().ok()
}

fn too_many(message: &str, retry_after_secs: u64) -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(header::RETRY_AFTER, retry_after_secs.to_string())],
        Json(serde_json::json!({
            "error": message,
            "retry_after_seconds": retry_after_secs,
        })),
    )
        .into_response()
}

/// Middleware for `POST /api/v1/pre-spend-check`. With no limiter attached
/// (tests that do not exercise it) the request passes through untouched.
pub async fn rate_limit(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let Some(limiter) = state.limiter() else {
        return next.run(request).await;
    };
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    let client = client_ip(request.headers(), peer, limiter.config().trust_proxy);

    // The cap is checked first so a refusal for load does not spend the
    // client's own allowance.
    let Some(_permit) = limiter.try_permit() else {
        return too_many("too many checks in flight", CONCURRENCY_RETRY_SECS);
    };
    if let Err(retry_after) = limiter.check(client, Instant::now()) {
        return too_many("rate limit exceeded", retry_after);
    }
    next.run(request).await
}

/// SHA-256 of the admin token. Only the digest is kept in memory, and both
/// sides of the comparison are digests, so the comparison time depends
/// neither on the token's length nor on how much of it matches.
pub fn admin_digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

/// Whether `presented` matches the configured digest. Every failure mode —
/// no token configured, no header, wrong scheme, wrong value — takes the
/// same path and gives the same answer.
pub fn admin_authorised(expected: Option<&[u8; 32]>, presented: Option<&str>) -> bool {
    let presented_digest = admin_digest(presented.unwrap_or(""));
    let configured = expected.is_some();
    let expected = expected.copied().unwrap_or([0u8; 32]);
    // Bitwise `&`, not `&&`: no early exit that would leak which part failed.
    constant_time_eq(&expected, &presented_digest) & configured & presented.is_some()
}

/// Middleware for `POST /api/v1/audit-counterparty`. Runs before the body is
/// read, so an unauthenticated caller learns nothing about the endpoint,
/// including whether the audit worker is attached.
pub async fn require_admin(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if admin_authorised(state.admin_digest(), bearer_token(request.headers())) {
        return next.run(request).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        Json(serde_json::json!({ "error": "unauthorized" })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        map
    }

    #[test]
    fn config_defaults_and_validation() {
        assert_eq!(
            LimitsConfig::from_vars(None, Some(""), None).unwrap(),
            LimitsConfig::default()
        );
        assert_eq!(
            LimitsConfig::from_vars(Some("5"), Some("2"), Some("1")).unwrap(),
            LimitsConfig {
                per_minute: 5,
                max_concurrent: 2,
                trust_proxy: true
            }
        );
        for bad in ["0", "-1", "abc", "10001", "1.5"] {
            assert!(
                LimitsConfig::from_vars(Some(bad), None, None).is_err(),
                "{bad}"
            );
        }
        for bad in ["0", "abc", "1001"] {
            assert!(
                LimitsConfig::from_vars(None, Some(bad), None).is_err(),
                "{bad}"
            );
        }
        for bad in ["true", "yes", "2"] {
            assert!(
                LimitsConfig::from_vars(None, None, Some(bad)).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn window_allows_n_then_refuses_then_recovers() {
        let limiter = Limiter::new(LimitsConfig {
            per_minute: 3,
            ..LimitsConfig::default()
        });
        let start = Instant::now();
        let client = ip("203.0.113.7");
        for _ in 0..3 {
            assert!(limiter.check(client, start).is_ok());
        }
        let wait = limiter
            .check(client, start + Duration::from_secs(10))
            .unwrap_err();
        assert!((49..=50).contains(&wait), "{wait}");
        assert!(
            limiter
                .check(client, start + Duration::from_secs(60))
                .is_ok()
        );
    }

    #[test]
    fn a_refused_request_does_not_extend_the_wait() {
        let limiter = Limiter::new(LimitsConfig {
            per_minute: 1,
            ..LimitsConfig::default()
        });
        let start = Instant::now();
        let client = ip("203.0.113.8");
        assert!(limiter.check(client, start).is_ok());
        for second in 1..30 {
            assert!(
                limiter
                    .check(client, start + Duration::from_secs(second))
                    .is_err()
            );
        }
        assert!(
            limiter
                .check(client, start + Duration::from_secs(60))
                .is_ok()
        );
    }

    #[test]
    fn clients_are_independent_and_ipv6_shares_a_slash_64() {
        let limiter = Limiter::new(LimitsConfig {
            per_minute: 1,
            ..LimitsConfig::default()
        });
        let now = Instant::now();
        assert!(limiter.check(ip("203.0.113.1"), now).is_ok());
        assert!(limiter.check(ip("203.0.113.2"), now).is_ok());
        assert!(limiter.check(ip("2001:db8:1:2::1"), now).is_ok());
        assert!(limiter.check(ip("2001:db8:1:2:ffff::9"), now).is_err());
        assert!(limiter.check(ip("2001:db8:1:3::1"), now).is_ok());
        // An IPv4-mapped IPv6 address is the same client as the IPv4 one.
        assert!(limiter.check(ip("::ffff:203.0.113.1"), now).is_err());
    }

    #[test]
    fn the_client_table_is_bounded() {
        let limiter = Limiter::new(LimitsConfig::default());
        let now = Instant::now();
        for n in 0..MAX_TRACKED_CLIENTS as u32 {
            let client = IpAddr::V4(Ipv4Addr::from(0x0a00_0000 + n));
            assert!(limiter.check(client, now).is_ok());
        }
        // Full of live clients: a new one is refused, a known one is not.
        assert!(limiter.check(ip("198.51.100.1"), now).is_err());
        assert!(
            limiter
                .check(IpAddr::V4(Ipv4Addr::from(0x0a00_0000)), now)
                .is_ok()
        );
        // Once the windows have lapsed the table purges and admits again.
        assert!(
            limiter
                .check(ip("198.51.100.1"), now + Duration::from_secs(61))
                .is_ok()
        );
        let size = limiter.seen.lock().unwrap().len();
        assert!(size <= MAX_TRACKED_CLIENTS, "{size}");
    }

    #[test]
    fn concurrency_cap_hands_out_exactly_n_permits() {
        let limiter = Limiter::new(LimitsConfig {
            max_concurrent: 2,
            ..LimitsConfig::default()
        });
        let first = limiter.try_permit().expect("first");
        let _second = limiter.try_permit().expect("second");
        assert!(limiter.try_permit().is_none());
        drop(first);
        assert!(limiter.try_permit().is_some());
    }

    #[test]
    fn forwarded_header_is_ignored_unless_a_proxy_is_trusted() {
        let peer: SocketAddr = "192.0.2.10:5555".parse().unwrap();
        let spoofed = headers(&[("x-forwarded-for", "198.51.100.99")]);
        assert_eq!(client_ip(&spoofed, Some(peer), false), ip("192.0.2.10"));
        assert_eq!(client_ip(&spoofed, Some(peer), true), ip("198.51.100.99"));
    }

    #[test]
    fn behind_a_proxy_only_the_rightmost_entry_counts() {
        let peer: SocketAddr = "192.0.2.10:5555".parse().unwrap();
        // The caller wrote the left entries; the proxy appended the last.
        let chain = headers(&[("x-forwarded-for", "1.1.1.1, 2.2.2.2, 203.0.113.50")]);
        assert_eq!(client_ip(&chain, Some(peer), true), ip("203.0.113.50"));
        let two_headers = headers(&[
            ("x-forwarded-for", "9.9.9.9"),
            ("x-forwarded-for", "203.0.113.51"),
        ]);
        assert_eq!(
            client_ip(&two_headers, Some(peer), true),
            ip("203.0.113.51")
        );
        // Unparseable or absent falls back to the peer, never to the header.
        for junk in ["not-an-ip", "", "203.0.113.5:8080"] {
            let h = headers(&[("x-forwarded-for", junk)]);
            assert_eq!(
                client_ip(&h, Some(peer), true),
                ip("192.0.2.10"),
                "{junk:?}"
            );
        }
        assert_eq!(
            client_ip(&HeaderMap::new(), Some(peer), true),
            ip("192.0.2.10")
        );
    }

    #[test]
    fn missing_peer_info_shares_one_bucket() {
        assert_eq!(
            client_ip(&HeaderMap::new(), None, false),
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        );
    }

    #[test]
    fn admin_check_accepts_only_the_exact_token() {
        let digest = admin_digest("correct horse battery staple");
        assert!(admin_authorised(
            Some(&digest),
            Some("correct horse battery staple")
        ));
        for wrong in [
            "",
            "correct horse battery stapl",
            "correct horse battery staple ",
            "x",
        ] {
            assert!(!admin_authorised(Some(&digest), Some(wrong)), "{wrong:?}");
        }
        assert!(!admin_authorised(Some(&digest), None));
        // Closed by default: no configured token authorises nothing, not
        // even the empty string.
        assert!(!admin_authorised(None, Some("")));
        assert!(!admin_authorised(None, Some("anything")));
        assert!(!admin_authorised(None, None));
    }

    #[test]
    fn bearer_parsing_is_strict_about_shape() {
        assert_eq!(
            bearer_token(&headers(&[("authorization", "Bearer abc")])),
            Some("abc")
        );
        assert_eq!(
            bearer_token(&headers(&[("authorization", "bearer abc")])),
            Some("abc")
        );
        for bad in ["Basic abc", "Bearer", "abc", ""] {
            assert_eq!(
                bearer_token(&headers(&[("authorization", bad)])),
                None,
                "{bad:?}"
            );
        }
        assert_eq!(bearer_token(&HeaderMap::new()), None);
    }
}
