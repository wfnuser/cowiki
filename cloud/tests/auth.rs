use cowiki_cloud::auth::{
    OAUTH_STATE_TTL, api_key_hash, random_secret, validate_loopback_callback, verify_api_key,
};
use std::time::{Duration, SystemTime};

#[test]
fn desktop_callback_accepts_only_an_exact_loopback_url() {
    assert!(validate_loopback_callback("http://127.0.0.1:39281/auth/callback").is_ok());
    for value in [
        "https://evil.example/auth/callback",
        "http://localhost:39281/auth/callback",
        "http://127.0.0.1/auth/callback",
        "http://127.0.0.1:39281/other",
        "http://127.0.0.1:39281/auth/callback?next=evil",
        "file:///tmp/callback",
    ] {
        assert!(
            validate_loopback_callback(value).is_err(),
            "accepted {value}"
        );
    }
}

#[test]
fn api_keys_are_peppered_and_compared_without_plaintext() {
    let pepper = "0123456789abcdef0123456789abcdef";
    let token = "cw_key_secret-value";
    let stored = api_key_hash(token, pepper);

    assert_ne!(stored.as_slice(), token.as_bytes());
    assert!(verify_api_key(token, pepper, &stored));
    assert!(!verify_api_key("cw_key_other", pepper, &stored));
    assert!(!verify_api_key(
        token,
        "fedcba9876543210fedcba9876543210",
        &stored
    ));
}

#[test]
fn generated_secrets_have_domain_specific_prefixes_and_entropy() {
    let first = random_secret("cw_state_");
    let second = random_secret("cw_state_");
    assert!(first.starts_with("cw_state_"));
    assert!(first.len() >= 50);
    assert_ne!(first, second);
}

#[test]
fn oauth_state_lifetime_is_ten_minutes() {
    assert_eq!(OAUTH_STATE_TTL, Duration::from_secs(600));
    let issued = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    assert!(issued + OAUTH_STATE_TTL > issued);
}
