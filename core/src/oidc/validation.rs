use std::net::IpAddr;

use adscope_store::OAuthClientType;
use anyhow::{Context, ensure};
use url::{Host, Url};

pub const OIDC_BODY_LIMIT_BYTES: usize = 16 * 1024;

pub fn validate_client_id(value: &str) -> anyhow::Result<()> {
    validate_char_count(value, 1, 128, "client_id")
}

pub fn validate_client_name(value: &str) -> anyhow::Result<()> {
    validate_char_count(value, 1, 100, "client name")
}

pub fn validate_scopes(value: &str) -> anyhow::Result<Vec<&str>> {
    validate_char_count(value, 1, 256, "scope")?;
    let scopes = value.split(' ').collect::<Vec<_>>();
    ensure!(
        !scopes.is_empty() && scopes.len() <= 4,
        "scope count must be 1..=4"
    );
    ensure!(
        scopes.iter().all(|scope| !scope.is_empty()),
        "scopes must use one space separators"
    );
    ensure!(scopes.contains(&"openid"), "openid scope is required");
    ensure!(
        scopes
            .iter()
            .all(|scope| matches!(*scope, "openid" | "profile" | "email" | "phone")),
        "unsupported scope"
    );
    let mut unique = scopes.clone();
    unique.sort_unstable();
    unique.dedup();
    ensure!(unique.len() == scopes.len(), "scopes must not repeat");
    Ok(scopes)
}

pub fn validate_state(value: &str) -> anyhow::Result<()> {
    validate_char_count(value, 1, 512, "state")
}

pub fn validate_nonce(value: &str) -> anyhow::Result<()> {
    validate_char_count(value, 1, 256, "nonce")
}

pub fn validate_code_challenge(value: &str) -> anyhow::Result<()> {
    ensure!(
        value.len() == 43,
        "code challenge must contain 43 characters"
    );
    ensure!(value.bytes().all(is_base64url), "invalid code challenge");
    Ok(())
}

pub fn validate_code_verifier(value: &str) -> anyhow::Result<()> {
    ensure!(
        (43..=128).contains(&value.len()),
        "code verifier length must be 43..=128"
    );
    ensure!(value.bytes().all(is_unreserved), "invalid code verifier");
    Ok(())
}

pub fn validate_response_mode(value: Option<&str>) -> anyhow::Result<()> {
    ensure!(
        value.is_none() || value == Some("query"),
        "unsupported response_mode"
    );
    Ok(())
}

pub fn validate_redirect_uris(
    client_type: OAuthClientType,
    redirect_uris: &[String],
    allow_insecure_web_loopback_redirects: bool,
) -> anyhow::Result<()> {
    ensure!(
        (1..=10).contains(&redirect_uris.len()),
        "redirect URI count must be 1..=10"
    );
    for redirect_uri in redirect_uris {
        ensure!(
            redirect_uri.len() <= 2048,
            "redirect URI exceeds 2048 bytes"
        );
        let url = parse_redirect_uri(redirect_uri)?;
        validate_redirect_shape(client_type, &url, allow_insecure_web_loopback_redirects)?;
    }
    Ok(())
}

pub fn validate_redirect_uri<'a>(
    client_type: OAuthClientType,
    registered: &[String],
    requested: &'a str,
    allow_insecure_web_loopback_redirects: bool,
) -> anyhow::Result<&'a str> {
    validate_redirect_uris(
        client_type,
        registered,
        allow_insecure_web_loopback_redirects,
    )?;
    ensure!(requested.len() <= 2048, "redirect URI exceeds 2048 bytes");
    let requested_url = parse_redirect_uri(requested)?;
    validate_redirect_shape(
        client_type,
        &requested_url,
        allow_insecure_web_loopback_redirects,
    )?;

    let matches = registered.iter().any(|registered| match client_type {
        OAuthClientType::Web => web_redirect_matches(registered, &requested_url),
        OAuthClientType::Desktop => desktop_redirect_matches(registered, &requested_url),
    });
    ensure!(matches, "redirect URI is not registered");
    Ok(requested)
}

fn validate_char_count(value: &str, min: usize, max: usize, label: &str) -> anyhow::Result<()> {
    let count = value.chars().count();
    ensure!(
        (min..=max).contains(&count),
        "{label} length must be {min}..={max}"
    );
    Ok(())
}

fn parse_redirect_uri(value: &str) -> anyhow::Result<Url> {
    let url = Url::parse(value).context("invalid redirect URI")?;
    ensure!(!url.cannot_be_a_base(), "redirect URI must be absolute");
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "redirect URI must not include userinfo"
    );
    ensure!(
        url.fragment().is_none(),
        "redirect URI must not include a fragment"
    );
    Ok(url)
}

fn validate_redirect_shape(
    client_type: OAuthClientType,
    url: &Url,
    allow_insecure_web_loopback_redirects: bool,
) -> anyhow::Result<()> {
    match client_type {
        OAuthClientType::Web => {
            if url.scheme() == "https" {
                ensure!(url.host().is_some(), "Web redirect must include a host");
                return Ok(());
            }
            ensure!(
                allow_insecure_web_loopback_redirects
                    && url.scheme() == "http"
                    && is_loopback_ip(url),
                "Web redirect must use HTTPS"
            );
        }
        OAuthClientType::Desktop => {
            ensure!(url.scheme() == "http", "Desktop redirect must use HTTP");
            ensure!(
                is_loopback_ip(url),
                "Desktop redirect must use a loopback IP"
            );
        }
    }
    Ok(())
}

fn web_redirect_matches(registered: &str, requested: &Url) -> bool {
    let Ok(registered) = Url::parse(registered) else {
        return false;
    };
    registered.scheme() == requested.scheme()
        && registered.host() == requested.host()
        && registered.port() == requested.port()
        && registered.path() == requested.path()
        && registered.query() == requested.query()
}

fn desktop_redirect_matches(registered: &str, requested: &Url) -> bool {
    let Ok(mut registered) = Url::parse(registered) else {
        return false;
    };
    let Some(requested_port) = requested.port() else {
        return false;
    };
    if registered.set_port(Some(requested_port)).is_err() {
        return false;
    }
    registered == *requested
}

fn is_loopback_ip(url: &Url) -> bool {
    match url.host() {
        Some(Host::Ipv4(address)) => IpAddr::V4(address).is_loopback(),
        Some(Host::Ipv6(address)) => IpAddr::V6(address).is_loopback(),
        _ => false,
    }
}

fn is_base64url(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

#[cfg(test)]
mod tests {
    use adscope_store::OAuthClientType;

    use super::{
        OIDC_BODY_LIMIT_BYTES, validate_client_id, validate_client_name, validate_code_challenge,
        validate_code_verifier, validate_nonce, validate_redirect_uri, validate_redirect_uris,
        validate_response_mode, validate_scopes, validate_state,
    };

    const WEB_CALLBACK: &str = "https://portal.example.com/oauth/callback?source=adscope";

    #[test]
    fn body_limit_is_sixteen_kibibytes() {
        assert_eq!(OIDC_BODY_LIMIT_BYTES, 16 * 1024);
    }

    #[test]
    fn client_id_must_be_non_empty_and_at_most_128_characters() {
        assert!(validate_client_id("client_web").is_ok());
        assert!(validate_client_id(&"a".repeat(128)).is_ok());
        assert!(validate_client_id("").is_err());
        assert!(validate_client_id(&"a".repeat(129)).is_err());
    }

    #[test]
    fn client_name_must_have_between_one_and_100_characters() {
        assert!(validate_client_name("Portal").is_ok());
        assert!(validate_client_name(&"界".repeat(100)).is_ok());
        assert!(validate_client_name("").is_err());
        assert!(validate_client_name(&"界".repeat(101)).is_err());
    }

    #[test]
    fn scopes_are_fixed_unique_and_include_openid() {
        assert_eq!(
            validate_scopes("openid profile email phone").unwrap(),
            ["openid", "profile", "email", "phone"]
        );
        assert!(validate_scopes("profile").is_err());
        assert!(validate_scopes("openid groups").is_err());
        assert!(validate_scopes("openid profile profile").is_err());
        assert!(validate_scopes("openid  profile").is_err());
        assert!(validate_scopes(&format!("openid {}", "x".repeat(250))).is_err());
    }

    #[test]
    fn authorization_value_lengths_are_bounded() {
        assert!(validate_state("state").is_ok());
        assert!(validate_state(&"s".repeat(512)).is_ok());
        assert!(validate_state("").is_err());
        assert!(validate_state(&"s".repeat(513)).is_err());

        assert!(validate_nonce("nonce").is_ok());
        assert!(validate_nonce(&"n".repeat(256)).is_ok());
        assert!(validate_nonce("").is_err());
        assert!(validate_nonce(&"n".repeat(257)).is_err());
    }

    #[test]
    fn s256_challenge_is_exactly_43_base64url_characters() {
        assert!(validate_code_challenge(&"A".repeat(43)).is_ok());
        assert!(validate_code_challenge(&"A".repeat(42)).is_err());
        assert!(validate_code_challenge(&"A".repeat(44)).is_err());
        assert!(validate_code_challenge(&format!("{}=", "A".repeat(42))).is_err());
        assert!(validate_code_challenge(&format!("{}+", "A".repeat(42))).is_err());
    }

    #[test]
    fn verifier_uses_rfc7636_unreserved_characters_and_length() {
        assert!(validate_code_verifier(&"a".repeat(43)).is_ok());
        assert!(validate_code_verifier(&"a".repeat(128)).is_ok());
        assert!(validate_code_verifier(&"a".repeat(42)).is_err());
        assert!(validate_code_verifier(&"a".repeat(129)).is_err());
        assert!(validate_code_verifier(&format!("{}+", "a".repeat(42))).is_err());
        assert!(validate_code_verifier(&format!("{}=", "a".repeat(42))).is_err());
        assert!(validate_code_verifier(&format!("{} ", "a".repeat(42))).is_err());
        assert!(validate_code_verifier(&format!("{}~", "a".repeat(42))).is_ok());
    }

    #[test]
    fn response_mode_is_absent_or_query() {
        assert!(validate_response_mode(None).is_ok());
        assert!(validate_response_mode(Some("query")).is_ok());
        assert!(validate_response_mode(Some("fragment")).is_err());
        assert!(validate_response_mode(Some("")).is_err());
    }

    #[test]
    fn web_redirect_requires_an_exact_registered_https_uri() {
        let registered = [WEB_CALLBACK.to_string()];
        assert_eq!(
            validate_redirect_uri(OAuthClientType::Web, &registered, WEB_CALLBACK, false).unwrap(),
            WEB_CALLBACK
        );
        assert!(
            validate_redirect_uri(
                OAuthClientType::Web,
                &registered,
                "https://portal.example.com/oauth/callback?source=other",
                false,
            )
            .is_err()
        );
        assert!(
            validate_redirect_uri(
                OAuthClientType::Web,
                &registered,
                "https://portal.example.com/oauth/callback/child?source=adscope",
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn web_redirect_compares_parsed_structure_and_rejects_field_changes() {
        let registered =
            ["https://PORTAL.example.com:8443/oauth/callback?source=adscope".to_string()];
        assert!(
            validate_redirect_uri(
                OAuthClientType::Web,
                &registered,
                "https://portal.example.com:8443/oauth/callback?source=adscope",
                false,
            )
            .is_ok()
        );

        for changed in [
            "http://portal.example.com:8443/oauth/callback?source=adscope",
            "https://other.example.com:8443/oauth/callback?source=adscope",
            "https://portal.example.com:9443/oauth/callback?source=adscope",
            "https://portal.example.com:8443/oauth/other?source=adscope",
            "https://portal.example.com:8443/oauth/callback?source=other",
        ] {
            assert!(
                validate_redirect_uri(OAuthClientType::Web, &registered, changed, false).is_err(),
                "unexpectedly accepted {changed}"
            );
        }
    }

    #[test]
    fn web_redirect_rejects_http_except_explicit_loopback_development() {
        let loopback = ["http://127.0.0.1:3000/callback".to_string()];
        assert!(validate_redirect_uris(OAuthClientType::Web, &loopback, false).is_err());
        assert!(validate_redirect_uris(OAuthClientType::Web, &loopback, true).is_ok());
        assert!(
            validate_redirect_uris(
                OAuthClientType::Web,
                &["http://portal.example.com/callback".to_string()],
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn desktop_redirect_only_allows_loopback_ip_and_variable_port() {
        let registered = ["http://127.0.0.1:41000/callback?source=adscope".to_string()];
        assert_eq!(
            validate_redirect_uri(
                OAuthClientType::Desktop,
                &registered,
                "http://127.0.0.1:51000/callback?source=adscope",
                false,
            )
            .unwrap(),
            "http://127.0.0.1:51000/callback?source=adscope"
        );
        assert!(
            validate_redirect_uri(
                OAuthClientType::Desktop,
                &registered,
                "http://127.0.0.2:51000/callback?source=adscope",
                false,
            )
            .is_err()
        );
        assert!(
            validate_redirect_uri(
                OAuthClientType::Desktop,
                &registered,
                "http://127.0.0.1:51000/other?source=adscope",
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn redirect_uris_reject_localhost_userinfo_fragment_and_invalid_shapes() {
        for uri in [
            "http://localhost:4000/callback",
            "http://user@127.0.0.1:4000/callback",
            "http://127.0.0.1:4000/callback#fragment",
            "/relative/callback",
        ] {
            assert!(
                validate_redirect_uris(OAuthClientType::Desktop, &[uri.to_string()], false)
                    .is_err(),
                "unexpectedly accepted {uri}"
            );
        }
    }

    #[test]
    fn redirect_uri_collection_and_value_lengths_are_bounded() {
        assert!(validate_redirect_uris(OAuthClientType::Web, &[], false).is_err());
        assert!(
            validate_redirect_uris(
                OAuthClientType::Web,
                &vec!["https://example.com/callback".to_string(); 11],
                false,
            )
            .is_err()
        );
        assert!(
            validate_redirect_uris(
                OAuthClientType::Web,
                &[format!("https://example.com/{}", "a".repeat(2049))],
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn desktop_redirect_supports_ipv6_loopback() {
        let registered = ["http://[::1]:41000/callback".to_string()];
        assert!(
            validate_redirect_uri(
                OAuthClientType::Desktop,
                &registered,
                "http://[::1]:51000/callback",
                false,
            )
            .is_ok()
        );
    }
}
