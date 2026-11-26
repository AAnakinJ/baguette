//! Creates the licence signing key pair, once.
//! The private key (JWK) goes straight into the OS keychain, never to a file or the screen;
//! the public key is printed, to put in `product.rs`. Running it again changes nothing and
//! prints the existing public key.
//! At deploy time the private key is read from the keychain and piped into
//! `wrangler secret put SIGNING_KEY` (see docs/phase-5-checklist.md).

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use ed25519_dalek::SigningKey;
use rand_core::{OsRng, RngCore};

/// Fixed on purpose (not product::KEYRING_SERVICE): renaming the app must not lose the key.
const KEYRING_SERVICE: &str = "baton-desktop";
const ENTRY: &str = "licence-signing-key-jwk";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, ENTRY)?;
    if let Ok(existing) = entry.get_password() {
        let jwk: serde_json::Value = serde_json::from_str(&existing)?;
        println!("A signing key already exists in the keychain.");
        println!("public key: {}", jwk["x"].as_str().unwrap_or("?"));
        return Ok(());
    }
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    let sk = SigningKey::from_bytes(&seed);
    let public = B64.encode(sk.verifying_key().to_bytes());
    let jwk = serde_json::json!({"kty": "OKP", "crv": "Ed25519", "d": B64.encode(seed), "x": public});
    entry.set_password(&jwk.to_string())?;
    seed.fill(0);
    println!("New signing key stored in the keychain ({KEYRING_SERVICE} / {ENTRY}).");
    println!("public key: {public}");
    Ok(())
}
