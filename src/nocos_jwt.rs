// NOCOS Connect session-token validation for hbbr (Phase 3.2a-2).
//
// Drop into nocos-connect-server's src/. Add `mod nocos_jwt;` in
// main.rs/lib.rs. See tools/hbbr-jwt-patch/README.md.
//
// Input: the `licence_key` string from a RequestRelay protobuf frame.
// Output: Result<SessionClaims, NocosJwtError> — accept on Ok, reject
// on Err. Non-JWT strings (stock RustDesk clients) are rejected with
// NotAJwt so the caller can fall through to the existing key check.
//
// External contract:
// - NOCOS_CONNECT_PUBLIC_KEY env var: PEM-encoded Ed25519 public key.
//   Must match NOCOS_CONNECT_SIGNING_PUBLIC_KEY on the NOCOS side.
// - Unset env var → all validation returns NotConfigured; hbbr falls
//   through to stock key-equality path. Zero impact on existing
//   deployments until the env var is set.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use once_cell::sync::Lazy;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Claims structure mirrors what NOCOS's core.connect_jwt.issue_session_token
// emits. Only fields we actually use are declared; extras are silently
// dropped (same posture as the Python side).
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SessionClaims {
    pub sid: String,
    pub sub: String,
    pub aud: String,
    pub exp: usize,
    pub iat: usize,
    #[serde(default)]
    pub relay_host: Option<String>,
    #[serde(default)]
    pub relay_pin: Option<String>,
}

// ---------------------------------------------------------------------------
// Error taxonomy — the caller distinguishes "this wasn't a JWT at all"
// (fall through to stock key check) from "this was a JWT but bad" (reject).
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum NocosJwtError {
    NotAJwt,            // didn't look like a JWT; caller should try stock path
    NotConfigured,      // NOCOS_CONNECT_PUBLIC_KEY unset; same as NotAJwt for caller
    InvalidSignature,
    InvalidClaims(String),
    Expired,
    Replayed,
}

// ---------------------------------------------------------------------------
// Key loading — cached once at startup. A panic here is intentional if
// the env var is malformed: better to fail fast than accept all tokens
// silently. But if the env var is unset, keep going — stock mode.
// ---------------------------------------------------------------------------

static DECODING_KEY: Lazy<Option<DecodingKey>> = Lazy::new(|| {
    match std::env::var("NOCOS_CONNECT_PUBLIC_KEY") {
        Ok(pem) if !pem.is_empty() => {
            match DecodingKey::from_ed_pem(pem.as_bytes()) {
                Ok(key) => {
                    log::info!("NOCOS Connect JWT validation enabled");
                    Some(key)
                }
                Err(e) => {
                    panic!(
                        "NOCOS_CONNECT_PUBLIC_KEY is set but not a valid Ed25519 PEM: {e}"
                    );
                }
            }
        }
        _ => {
            log::info!(
                "NOCOS_CONNECT_PUBLIC_KEY unset — JWT validation disabled, stock hbbr auth only"
            );
            None
        }
    }
});

// ---------------------------------------------------------------------------
// Replay cache — bounded, process-local, expiry-swept. Keys: sid.
// A restart flushes it (acceptable given token TTLs are 60s).
// ---------------------------------------------------------------------------

const REPLAY_CACHE_MAX: usize = 10_000;

struct ReplayCache {
    // sid -> when the entry may be evicted (token's exp + small buffer)
    seen: HashMap<String, Instant>,
}

impl ReplayCache {
    fn new() -> Self {
        Self { seen: HashMap::new() }
    }

    fn record(&mut self, sid: String, expires_at: Instant) -> bool {
        // Sweep expired entries opportunistically (keeps the cache bounded
        // without a background task).
        if self.seen.len() >= REPLAY_CACHE_MAX {
            let now = Instant::now();
            self.seen.retain(|_, t| *t > now);
        }
        if self.seen.contains_key(&sid) {
            return false;
        }
        self.seen.insert(sid, expires_at);
        true
    }
}

static REPLAY: Lazy<Mutex<ReplayCache>> = Lazy::new(|| Mutex::new(ReplayCache::new()));

// ---------------------------------------------------------------------------
// The entry point hbbr calls.
// ---------------------------------------------------------------------------

pub fn verify(licence_key: &str) -> Result<SessionClaims, NocosJwtError> {
    if !licence_key.starts_with("eyJ") {
        return Err(NocosJwtError::NotAJwt);
    }
    let key = match DECODING_KEY.as_ref() {
        Some(k) => k,
        None => return Err(NocosJwtError::NotConfigured),
    };

    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.set_audience(&["hbbr"]);
    validation.leeway = 2;
    // Required-claims: jsonwebtoken doesn't have a required-claims list
    // by default, we check shape manually in SessionClaims.
    validation.required_spec_claims = ["exp", "iat", "aud", "sub"]
        .into_iter()
        .map(String::from)
        .collect();

    let data = match decode::<SessionClaims>(licence_key, key, &validation) {
        Ok(d) => d,
        Err(e) => {
            return Err(match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => NocosJwtError::Expired,
                jsonwebtoken::errors::ErrorKind::InvalidSignature => NocosJwtError::InvalidSignature,
                _ => NocosJwtError::InvalidClaims(format!("{e}")),
            });
        }
    };

    let claims = data.claims;
    if claims.sid.is_empty() {
        return Err(NocosJwtError::InvalidClaims("empty sid".into()));
    }

    let expires_at = Instant::now() + Duration::from_secs(120);
    let mut cache = REPLAY.lock().expect("replay cache poisoned");
    if !cache.record(claims.sid.clone(), expires_at) {
        return Err(NocosJwtError::Replayed);
    }

    Ok(claims)
}

// ---------------------------------------------------------------------------
// Unit tests. Run with `cargo test --lib nocos_jwt`.
// Requires a test-only keypair; generate once with scripts/gen-test-keys.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde::Serialize;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Fixed test keypair — OK to ship since it's only used inside
    // #[cfg(test)]. Generate-replace freely.
    const TEST_PRIV_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MC4CAQAwBQYDK2VwBCIEIKKRBwmXX1qBpVaUHcjKkHkPHnRSuKqcWJ2Qq27oHi8f\n\
-----END PRIVATE KEY-----";
    const TEST_PUB_PEM: &str = "-----BEGIN PUBLIC KEY-----\n\
MCowBQYDK2VwAyEAHYPhE9S3qM2bNDWrgF0N0NcEy3GbsP3RCRp4rDaH8hA=\n\
-----END PUBLIC KEY-----";

    #[derive(Serialize)]
    struct TestClaims<'a> {
        sid: &'a str,
        sub: &'a str,
        aud: &'a str,
        exp: usize,
        iat: usize,
    }

    fn now() -> usize {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as usize
    }

    fn mint(aud: &str, sid: &str, exp_offset: i64) -> String {
        let now_s = now();
        let claims = TestClaims {
            sid,
            sub: "test-agent",
            aud,
            iat: now_s,
            exp: ((now_s as i64) + exp_offset) as usize,
        };
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".into());
        let enc_key = EncodingKey::from_ed_pem(TEST_PRIV_PEM.as_bytes()).unwrap();
        encode(&header, &claims, &enc_key).unwrap()
    }

    fn verify_with_test_key(token: &str) -> Result<SessionClaims, NocosJwtError> {
        let key = DecodingKey::from_ed_pem(TEST_PUB_PEM.as_bytes()).unwrap();
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_audience(&["hbbr"]);
        validation.leeway = 2;
        let data = decode::<SessionClaims>(token, &key, &validation)
            .map_err(|e| NocosJwtError::InvalidClaims(format!("{e}")))?;
        Ok(data.claims)
    }

    #[test]
    fn roundtrip_happy_path() {
        let tok = mint("hbbr", "s1", 60);
        let claims = verify_with_test_key(&tok).unwrap();
        assert_eq!(claims.sid, "s1");
        assert_eq!(claims.aud, "hbbr");
    }

    #[test]
    fn wrong_audience_rejected() {
        let tok = mint("nocos-connect", "s2", 60);
        assert!(verify_with_test_key(&tok).is_err());
    }

    #[test]
    fn expired_token_rejected() {
        let tok = mint("hbbr", "s3", -60);
        assert!(verify_with_test_key(&tok).is_err());
    }

    #[test]
    fn non_jwt_reports_not_a_jwt() {
        assert!(matches!(verify("some-stock-key"), Err(NocosJwtError::NotAJwt)));
    }

    #[test]
    fn replay_cache_rejects_second_use() {
        let mut cache = ReplayCache::new();
        let exp = Instant::now() + Duration::from_secs(60);
        assert!(cache.record("sid-a".into(), exp));
        assert!(!cache.record("sid-a".into(), exp));
    }
}
