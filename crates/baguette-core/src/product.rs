//! Single place for the product identity. Final name chosen on 2026-11-26: "Baguette"
//! (the conductor's baton, in French). Change it here only.

pub const PRODUCT_NAME: &str = "Baguette";
pub const PRODUCT_SLUG: &str = "baguette";
/// Service name used in the OS keychain (Windows Credential Manager / Secret Service).
pub const KEYRING_SERVICE: &str = "baguette-desktop";
/// Oldest Claude Code version the adapter is tested against.
pub const MIN_CLAUDE_VERSION: (u32, u32, u32) = (2, 1, 200);

/// Early access: until paid plans open, every Pro feature is free for everyone and the app
/// makes no licence network call at all. Turning it off brings back the 14-day trial and keys.
pub const EARLY_ACCESS: bool = true;

/// Public half of the licence signing key (Ed25519, base64url). The private half never
/// leaves the operator's keychain and the licence server. Created on 2026-11-25.
pub const LICENCE_PUBLIC_KEY: &str = "_eOAeOB_z3iIsLbGRFjlbmvxztSB8gFqZASbaB7CiH4";
/// Licence server (Cloudflare Worker) and website. Free subdomains until a domain is bought;
/// set for real at deploy time (docs/phase-5-checklist.md).
pub const API_BASE: &str = "https://baguette-licence.workers.dev";
pub const SITE_URL: &str = "https://aanakinj.github.io/baguette";
/// Where releases and the source code live (set when the public repository is created).
pub const REPO_URL: &str = "https://github.com/AAnakinJ/baguette";
