use anyhow::{anyhow, Result};
use base64::Engine;
use serde::de::DeserializeOwned;

pub fn decode_payload<T: DeserializeOwned>(token: &str) -> Result<T> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return Err(anyhow!("invalid JWT format"));
    }

    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| anyhow!("JWT payload Base64 decode failed: {}", e))?;

    serde_json::from_slice(&payload).map_err(|e| anyhow!("JWT payload JSON parse failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Claims {
        sub: String,
    }

    #[test]
    fn test_decode_payload_success() {
        let payload =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"sub":"user_123"}"#);
        let token = format!("header.{}.sig", payload);
        let claims: Claims = decode_payload(&token).unwrap();
        assert_eq!(claims.sub, "user_123");
    }

    #[test]
    fn test_decode_payload_invalid_format() {
        let result: Result<Claims> = decode_payload("badtoken");
        assert!(result.is_err());
    }
}
