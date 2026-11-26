//! Pro licences and remote flags, as small signed tokens.
//!
//! The licence server (Cloudflare Worker) signs; the app only verifies, with the public key
//! in `product.rs`. A token is `base64url(payload JSON) "." base64url(Ed25519 signature)`, the
//! signature covering the payload bytes exactly as sent. Nothing secret lives in the app, and
//! the network calls are made by the interface: a forged or replayed answer cannot pass here.

use crate::entitlements::Plan;
use crate::store::Store;
use crate::{Error, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use ed25519_dalek::Signature;
pub use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Keychain entries (the token proves the plan, the key lets the app refresh it).
pub const TOKEN_SECRET: &str = "licence_token";
pub const KEY_SECRET: &str = "licence_key";
pub const TRIAL_DAYS: i64 = 14;
const DAY: i64 = 86_400;
const LOCAL_TRIAL_KEY: &str = "trial.local_started";
const TRIAL_REFUSED_KEY: &str = "trial.refused";
const FLAGS_IAT_KEY: &str = "flags.iat";

fn bad(what: &str) -> Error {
    Error::Invalid(format!("licence_{what}"))
}

pub fn public_key(b64: &str) -> Result<VerifyingKey> {
    let raw = B64.decode(b64.trim()).map_err(|_| bad("public_key"))?;
    let arr: [u8; 32] = raw.try_into().map_err(|_| bad("public_key"))?;
    VerifyingKey::from_bytes(&arr).map_err(|_| bad("public_key"))
}

/// Checks the signature and returns the payload. Strict: no padding, no extra parts.
pub fn open(token: &str, key: &VerifyingKey) -> Result<serde_json::Value> {
    let (payload, sig) = token.trim().split_once('.').ok_or_else(|| bad("format"))?;
    if payload.is_empty() || sig.contains('.') || token.len() > 4096 {
        return Err(bad("format"));
    }
    let payload_bytes = B64.decode(payload).map_err(|_| bad("format"))?;
    let sig_bytes: [u8; 64] = B64.decode(sig).map_err(|_| bad("format"))?.try_into().map_err(|_| bad("format"))?;
    key.verify_strict(&payload_bytes, &Signature::from_bytes(&sig_bytes)).map_err(|_| bad("signature"))?;
    serde_json::from_slice(&payload_bytes).map_err(|_| bad("format"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Trial,
    Pro,
    Lifetime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claims {
    pub v: u32,
    pub typ: String,
    pub kind: Kind,
    /// Licence id (or trial id). Never the key itself.
    pub lid: String,
    /// Device hash the token was issued for.
    pub dev: String,
    pub iat: i64,
    pub exp: i64,
}

/// A licence token valid for this device right now.
pub fn verify_licence(token: &str, key: &VerifyingKey, device: &str, now: i64) -> Result<Claims> {
    let c: Claims = serde_json::from_value(open(token, key)?).map_err(|_| bad("format"))?;
    if c.v != 1 || c.typ != "licence" {
        return Err(bad("format"));
    }
    if c.dev != device {
        return Err(bad("device"));
    }
    if now >= c.exp {
        return Err(bad("expired"));
    }
    if c.iat > now + DAY {
        return Err(bad("clock"));
    }
    Ok(c)
}

/// Remote switches, read at start-up (Phase 1: the subscription mode can be turned off).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flags {
    pub v: u32,
    pub typ: String,
    pub iat: i64,
    pub exp: i64,
    pub subscription_mode: bool,
    #[serde(default)]
    pub message: Option<String>,
}

pub fn verify_flags(token: &str, key: &VerifyingKey, now: i64) -> Result<Flags> {
    let f: Flags = serde_json::from_value(open(token, key)?).map_err(|_| bad("format"))?;
    if f.v != 1 || f.typ != "flags" {
        return Err(bad("format"));
    }
    if now >= f.exp {
        return Err(bad("expired"));
    }
    Ok(f)
}

/// Applies signed flags. An older token than the last one applied is ignored (no replay
/// of a stale "on" after an "off").
pub fn apply_flags(store: &Store, token: &str, key: &VerifyingKey, now: i64) -> Result<Flags> {
    let f = verify_flags(token, key, now)?;
    let last: i64 = store.get_value(FLAGS_IAT_KEY)?.and_then(|v| v.parse().ok()).unwrap_or(0);
    if f.iat < last {
        return Err(bad("stale"));
    }
    store.set_value("flag.subscription_mode", if f.subscription_mode { "on" } else { "off" })?;
    store.set_value(FLAGS_IAT_KEY, &f.iat.to_string())?;
    Ok(f)
}

// ---------- device ----------

#[cfg(windows)]
fn machine_id() -> Option<String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("reg")
        .args(["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().find(|l| l.contains("MachineGuid")).and_then(|l| l.split_whitespace().last()).map(str::to_string)
}

#[cfg(not(windows))]
fn machine_id() -> Option<String> {
    ["/etc/machine-id", "/var/lib/dbus/machine-id"].iter().find_map(|p| std::fs::read_to_string(p).ok()).map(|s| s.trim().to_string())
}

/// Anonymous, stable id of this computer: a hash, never the raw machine id. Falls back to a
/// random id kept in the database when the system gives none.
pub fn device_id(store: &Store) -> Result<String> {
    let raw = match machine_id().filter(|m| m.len() >= 8) {
        Some(m) => m,
        None => match store.get_value("device.id")? {
            Some(v) => v,
            None => {
                let v = uuid::Uuid::new_v4().to_string();
                store.set_value("device.id", &v)?;
                v
            }
        },
    };
    let digest = Sha256::digest(format!("{}:device:{raw}", crate::product::PRODUCT_SLUG).as_bytes());
    Ok(digest.iter().take(16).map(|b| format!("{b:02x}")).collect())
}

// ---------- plan ----------

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    pub plan: Plan,
    /// trial, pro, lifetime, or none.
    pub kind: Option<Kind>,
    /// When the current right ends (trial end, or token expiry for Pro). Unix seconds.
    pub until: Option<i64>,
    pub days_left: Option<i64>,
    /// The local trial was refused by the server (already used on this computer).
    pub trial_refused: bool,
    pub has_key: bool,
}

/// First launch starts the 14-day trial, without card or account.
pub fn ensure_local_trial(store: &Store, now: i64) -> Result<i64> {
    match store.get_value(LOCAL_TRIAL_KEY)?.and_then(|v| v.parse().ok()) {
        Some(t) => Ok(t),
        None => {
            store.set_value(LOCAL_TRIAL_KEY, &now.to_string())?;
            Ok(now)
        }
    }
}

/// The server said this computer already had its trial: no local trial any more.
pub fn refuse_trial(store: &Store) -> Result<()> {
    store.set_value(TRIAL_REFUSED_KEY, "1")
}

pub fn status(store: &Store, token: Option<&str>, has_key: bool, key: &VerifyingKey, device: &str, now: i64) -> Result<Status> {
    let days = |until: i64| ((until - now) as f64 / DAY as f64).ceil().max(0.0) as i64;
    if let Some(c) = token.and_then(|t| verify_licence(t, key, device, now).ok()) {
        let plan = if c.kind == Kind::Trial { Plan::Trial } else { Plan::Pro };
        return Ok(Status { plan, kind: Some(c.kind), until: Some(c.exp), days_left: Some(days(c.exp)), trial_refused: false, has_key });
    }
    let refused = store.get_value(TRIAL_REFUSED_KEY)?.is_some();
    let started = ensure_local_trial(store, now)?;
    let end = started + TRIAL_DAYS * DAY;
    if !refused && now < end {
        return Ok(Status { plan: Plan::Trial, kind: Some(Kind::Trial), until: Some(end), days_left: Some(days(end)), trial_refused: false, has_key });
    }
    Ok(Status { plan: Plan::Community, kind: None, until: None, days_left: None, trial_refused: refused, has_key })
}

/// "ABCD-…-WXYZ" -> "ABCD-…-WXYZ" with the middle hidden, for display.
pub fn mask_key(key: &str) -> String {
    let parts: Vec<&str> = key.split('-').collect();
    match parts.as_slice() {
        [first, .., last] if parts.len() > 2 => format!("{first}-••••-{last}"),
        _ => "••••".into(),
    }
}

/// Licence keys look like "PRO-7KQ2-M9XD-4TJH-WC3P": letters and digits without 0/O/1/I.
pub fn looks_like_key(key: &str) -> bool {
    let k = key.trim();
    let parts: Vec<&str> = k.split('-').collect();
    parts.len() == 5
        && matches!(parts[0], "PRO" | "LIFE")
        && parts[1..].iter().all(|p| p.len() == 4 && p.chars().all(|c| "23456789ABCDEFGHJKLMNPQRSTUVWXYZ".contains(c)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn keys() -> (SigningKey, VerifyingKey) {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let vk = sk.verifying_key();
        (sk, vk)
    }

    fn sign(sk: &SigningKey, payload: &serde_json::Value) -> String {
        let bytes = serde_json::to_vec(payload).unwrap();
        format!("{}.{}", B64.encode(&bytes), B64.encode(sk.sign(&bytes).to_bytes()))
    }

    fn claims(kind: &str, dev: &str, exp: i64) -> serde_json::Value {
        serde_json::json!({"v": 1, "typ": "licence", "kind": kind, "lid": "lic_1", "dev": dev, "iat": 1_000, "exp": exp})
    }

    #[test]
    fn valid_token_for_this_device() {
        let (sk, vk) = keys();
        let c = verify_licence(&sign(&sk, &claims("pro", "dev1", 5_000)), &vk, "dev1", 2_000).unwrap();
        assert_eq!(c.kind, Kind::Pro);
    }

    #[test]
    fn rejects_forgery_other_device_expiry_and_garbage() {
        let (sk, vk) = keys();
        let token = sign(&sk, &claims("pro", "dev1", 5_000));
        assert!(matches!(verify_licence(&token, &vk, "dev2", 2_000), Err(Error::Invalid(e)) if e == "licence_device"));
        assert!(matches!(verify_licence(&token, &vk, "dev1", 5_000), Err(Error::Invalid(e)) if e == "licence_expired"));
        // Payload changed after signing ("pro" -> "lifetime"): refused.
        let (_, sig) = token.split_once('.').unwrap();
        let forged = format!("{}.{sig}", B64.encode(serde_json::to_vec(&claims("lifetime", "dev1", 9_999_999)).unwrap()));
        assert!(matches!(verify_licence(&forged, &vk, "dev1", 2_000), Err(Error::Invalid(e)) if e == "licence_signature"));
        // Signed by another key: refused.
        let other = SigningKey::from_bytes(&[9u8; 32]);
        assert!(verify_licence(&sign(&other, &claims("pro", "dev1", 5_000)), &vk, "dev1", 2_000).is_err());
        for junk in ["", ".", "abc", "a.b.c", "!!!.???"] {
            assert!(verify_licence(junk, &vk, "dev1", 2_000).is_err(), "{junk}");
        }
    }

    #[test]
    fn flags_cannot_be_replayed() {
        let (sk, vk) = keys();
        let st = Store::open_in_memory().unwrap();
        let flags = |iat: i64, on: bool| sign(&sk, &serde_json::json!({"v": 1, "typ": "flags", "iat": iat, "exp": iat + 1_000, "subscription_mode": on}));
        apply_flags(&st, &flags(100, false), &vk, 150).unwrap();
        assert_eq!(st.get_value("flag.subscription_mode").unwrap().as_deref(), Some("off"));
        assert!(apply_flags(&st, &flags(50, true), &vk, 150).is_err(), "an older 'on' cannot undo a newer 'off'");
        assert_eq!(st.get_value("flag.subscription_mode").unwrap().as_deref(), Some("off"));
        apply_flags(&st, &flags(200, true), &vk, 250).unwrap();
        assert_eq!(st.get_value("flag.subscription_mode").unwrap().as_deref(), Some("on"));
        // A licence token is not a flags token.
        assert!(verify_flags(&sign(&sk, &claims("pro", "d", 9_000)), &vk, 100).is_err());
    }

    #[test]
    fn trial_then_community_then_pro() {
        let (sk, vk) = keys();
        let st = Store::open_in_memory().unwrap();
        let s = status(&st, None, false, &vk, "dev1", 0).unwrap();
        assert_eq!((s.plan, s.days_left), (Plan::Trial, Some(14)));
        let s = status(&st, None, false, &vk, "dev1", 13 * DAY + 1).unwrap();
        assert_eq!((s.plan, s.days_left), (Plan::Trial, Some(1)));
        assert_eq!(status(&st, None, false, &vk, "dev1", 14 * DAY).unwrap().plan, Plan::Community);
        let pro = sign(&sk, &claims("pro", "dev1", 20 * DAY));
        assert_eq!(status(&st, Some(&pro), true, &vk, "dev1", 15 * DAY).unwrap().plan, Plan::Pro);
        // A token for another computer does not count.
        assert_eq!(status(&st, Some(&pro), true, &vk, "dev2", 15 * DAY).unwrap().plan, Plan::Community);
    }

    #[test]
    fn refused_trial_ends_the_local_one() {
        let (_, vk) = keys();
        let st = Store::open_in_memory().unwrap();
        refuse_trial(&st).unwrap();
        let s = status(&st, None, false, &vk, "dev1", 0).unwrap();
        assert_eq!(s.plan, Plan::Community);
        assert!(s.trial_refused);
    }

    #[test]
    fn device_id_is_a_stable_hash() {
        let st = Store::open_in_memory().unwrap();
        let a = device_id(&st).unwrap();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(a, device_id(&st).unwrap());
    }

    #[test]
    fn keys_are_recognised_and_masked() {
        assert!(looks_like_key("PRO-7KQ2-M9XD-4TJH-WC3P"));
        assert!(looks_like_key(" LIFE-7KQ2-M9XD-4TJH-WC3P "));
        assert!(!looks_like_key("PRO-7KQ2-M9XD-4TJH-WC3O"), "no O");
        assert!(!looks_like_key("sk-ant-api03-xxx"));
        assert_eq!(mask_key("PRO-7KQ2-M9XD-4TJH-WC3P"), "PRO-••••-WC3P");
    }

    #[test]
    fn the_shipped_public_key_is_valid() {
        assert!(public_key(crate::product::LICENCE_PUBLIC_KEY).is_ok());
        assert!(public_key("not-a-key").is_err());
    }

    /// Same token as `server/test/fixture.ts` produces (Ed25519 is deterministic): proves the
    /// Worker and the app agree on the format, byte for byte.
    #[test]
    fn server_fixture_verifies() {
        let fixture = include_str!("../../../server/test/fixture-token.txt").trim();
        let (_, vk) = keys();
        let c = verify_licence(fixture, &vk, "0123456789abcdef0123456789abcdef", 1_800_000_000).unwrap();
        assert_eq!((c.kind, c.lid.as_str()), (Kind::Pro, "lic_fixture"));
    }
}
