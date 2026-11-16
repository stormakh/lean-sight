//! Secrets at rest (data source `secure_json_data`). AES-256-GCM with a key derived from `[security] secret_key`.
//! The stored text is base64(nonce || ciphertext || tag) of a JSON object of field -> plain value.
//! Different from Grafana (envelope encryption with data keys): the database format is ours.
use json::Json;

pub fn key_from_secret(secret_key: &str) -> [u8; 32] {
    let mut h = crypto::Sha256::new();
    h.update(b"grafana-rs secrets v1\0");
    h.update(secret_key.as_bytes());
    h.finish()
}

/// Encrypts a JSON object of secure fields. An empty object stores as "".
pub fn encrypt_map(secret_key: &str, fields: &Json) -> String {
    match fields.as_obj() {
        Some(o) if !o.is_empty() => {}
        _ => return String::new(),
    }
    let k = key_from_secret(secret_key);
    crypto::base64(&crypto::seal(fields.stringify().as_bytes(), &k))
}

/// Decrypts what `encrypt_map` stored. "" or a bad blob gives an empty object.
pub fn decrypt_map(secret_key: &str, stored: &str) -> Json {
    if stored.is_empty() {
        return Json::obj();
    }
    let k = key_from_secret(secret_key);
    let raw = match crypto::base64_decode(stored) {
        Some(r) => r,
        None => return Json::obj(),
    };
    match crypto::open(&raw, &k) {
        Ok(p) => Json::parse_bytes(&p).unwrap_or_else(|_| Json::obj()),
        Err(_) => Json::obj(),
    }
}

/// `secureJsonFields`: each stored field name mapped to true.
pub fn secure_fields(secret_key: &str, stored: &str) -> Json {
    let m = decrypt_map(secret_key, stored);
    let mut out = Json::obj();
    if let Some(o) = m.as_obj() {
        for (k, _) in o.iter() {
            out.set(k, true);
        }
    }
    out
}
