//! Canonical JSON and Ed25519 checks for the extension registry.
//! The private signing key is never present in this crate.

use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use serde_json::Value;

pub const REGISTRY_SCHEMA: &str = "menzies-extension-registry/1";
pub const REGISTRY_DOMAIN: &str = "menzies-extension-registry/1";
pub const ARTIFACT_DOMAIN: &str = "menzies-extension-artifact/1";
pub const FACE_DOMAIN: &str = "menzies-gallery-face-extension/1";
pub const KEY_ID: &str = "menzies-extensions-ed25519-1";

const PUBLISHER_KEY_JSON: &str = include_str!("../extensions/publisher-key.json");

#[derive(Debug)]
pub enum SignError {
    BadKey,
    BadSignature,
    Canonical,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublisherKeyFile {
    key_id: String,
    algorithm: String,
    public_key: String,
}

fn publisher_key() -> Result<VerifyingKey, SignError> {
    let file: PublisherKeyFile =
        serde_json::from_str(PUBLISHER_KEY_JSON).map_err(|_| SignError::BadKey)?;
    if file.key_id != KEY_ID || file.algorithm != "ed25519" {
        return Err(SignError::BadKey);
    }
    let bytes = STANDARD
        .decode(file.public_key.trim())
        .map_err(|_| SignError::BadKey)?;
    let array: [u8; 32] = bytes.try_into().map_err(|_| SignError::BadKey)?;
    VerifyingKey::from_bytes(&array).map_err(|_| SignError::BadKey)
}

pub fn parse_semver(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    if version.chars().any(|ch| ch == '-' || ch == '+') {
        return None;
    }
    if version
        .split('.')
        .any(|part| part.len() > 1 && part.starts_with('0') || part.is_empty() || part.len() > 9)
    {
        return None;
    }
    Some((major, minor, patch))
}

pub fn canonical_json(value: &Value) -> Result<String, SignError> {
    let mut out = String::new();
    write_canonical(&mut out, value)?;
    Ok(out)
}

fn write_canonical(out: &mut String, value: &Value) -> Result<(), SignError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => {
            if let Some(value) = number.as_u64() {
                out.push_str(&value.to_string());
            } else if let Some(value) = number.as_i64() {
                if value < 0 {
                    return Err(SignError::Canonical);
                }
                out.push_str(&value.to_string());
            } else {
                return Err(SignError::Canonical);
            }
        }
        Value::String(text) => {
            out.push('"');
            for ch in text.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\u{0008}' => out.push_str("\\b"),
                    '\u{000c}' => out.push_str("\\f"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    ch if (ch as u32) < 0x20 => {
                        out.push_str(&format!("\\u{:04x}", ch as u32));
                    }
                    ch => out.push(ch),
                }
            }
            out.push('"');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(out, item)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(out, &Value::String(key.clone()))?;
                out.push(':');
                write_canonical(out, item)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn verify_message(
    domain: &str,
    canonical_body: &str,
    signature_b64: &str,
) -> Result<(), SignError> {
    let key = publisher_key()?;
    let sig_bytes = STANDARD
        .decode(signature_b64.trim())
        .map_err(|_| SignError::BadSignature)?;
    let signature = Signature::from_slice(&sig_bytes).map_err(|_| SignError::BadSignature)?;
    let mut message = Vec::with_capacity(domain.len() + 1 + canonical_body.len());
    message.extend_from_slice(domain.as_bytes());
    message.push(b'\n');
    message.extend_from_slice(canonical_body.as_bytes());
    key.verify(&message, &signature)
        .map_err(|_| SignError::BadSignature)
}

pub fn verify_registry(canonical_body: &str, signature_b64: &str) -> Result<(), SignError> {
    verify_message(REGISTRY_DOMAIN, canonical_body, signature_b64)
}

pub fn verify_artifact(bytes: &[u8], signature_b64: &str) -> Result<(), SignError> {
    let key = publisher_key()?;
    let sig_bytes = STANDARD
        .decode(signature_b64.trim())
        .map_err(|_| SignError::BadSignature)?;
    let signature = Signature::from_slice(&sig_bytes).map_err(|_| SignError::BadSignature)?;
    let mut message = Vec::with_capacity(ARTIFACT_DOMAIN.len() + 1 + bytes.len());
    message.extend_from_slice(ARTIFACT_DOMAIN.as_bytes());
    message.push(b'\n');
    message.extend_from_slice(bytes);
    key.verify(&message, &signature)
        .map_err(|_| SignError::BadSignature)
}

pub fn verify_domain(
    domain: &str,
    canonical_body: &str,
    signature_b64: &str,
) -> Result<(), SignError> {
    verify_message(domain, canonical_body, signature_b64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;
    use ed25519_dalek::SigningKey;

    const SAMPLE_BODY: &str = r#"{"extensions":[{"appId":"gallery-desktop-menzies","availability":"unavailable","defaultUpdateMode":"manual","id":"face-recognition","name":"Face recognition","platforms":["linux","darwin","windows"],"summary":"Optional Gallery add-on that groups faces on this computer after you install it. No package is published yet, and photos are never uploaded.","unavailableReason":"artifact_not_published"}],"issuedAt":"2026-10-12T05:30:00Z","keyId":"menzies-extensions-ed25519-1","revoked":[],"schema":"menzies-extension-registry/1","sequence":1}"#;
    const SAMPLE_SIG: &str =
        "wdIWmLyi/6xZa9hBXxJSoPdw9FXbIN5wvpm0EB29Bs6K2E2UOHOaDJ1O0IsV2A73f6MyRtuU6IjOaFTcyvHYAg==";

    #[test]
    fn canonical_matches_the_website_vector() {
        let value: Value = serde_json::from_str(SAMPLE_BODY).expect("json");
        let canonical = canonical_json(&value).expect("canonical");
        assert_eq!(canonical, SAMPLE_BODY);
        verify_registry(SAMPLE_BODY, SAMPLE_SIG).expect("pinned signature");
        let mut bad = SAMPLE_SIG.to_string();
        bad.replace_range(0..1, "A");
        assert!(verify_registry(SAMPLE_BODY, &bad).is_err());
    }

    #[test]
    fn escapes_strings_like_the_website() {
        let value = serde_json::json!({"b": "a\"b", "a": [1, true, null]});
        let canonical = canonical_json(&value).expect("canonical");
        assert_eq!(canonical, r#"{"a":[1,true,null],"b":"a\"b"}"#);
    }

    #[test]
    fn artifact_prefix_does_not_match_a_raw_signature() {
        let signing = SigningKey::from_bytes(&[9u8; 32]);
        let bytes = b"package-bytes";
        let mut message = Vec::new();
        message.extend_from_slice(ARTIFACT_DOMAIN.as_bytes());
        message.push(b'\n');
        message.extend_from_slice(bytes);
        let signature = signing.sign(&message);
        let encoded = STANDARD.encode(signature.to_bytes());
        assert!(verify_artifact(bytes, &encoded).is_err());
        assert!(parse_semver("1.2.3").is_some());
        assert!(parse_semver("01.2.3").is_none());
        assert!(parse_semver("1.2").is_none());
    }
}
