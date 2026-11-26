//! Operator secrets in the OS keychain instead of files (Stripe keys for tests and deploys).
//!   echo VALUE | cargo run -q -p baguette-core --example secret -- set NAME
//!   cargo run -q -p baguette-core --example secret -- get NAME      (prints the value)
//!   cargo run -q -p baguette-core --example secret -- delete NAME

use std::io::Read;

/// Separate from the app's own entries ("baguette-desktop").
const SERVICE: &str = "baton-operator";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, name) = match args.as_slice() {
        [c, n] => (c.as_str(), n.as_str()),
        _ => return Err("usage: secret set|get|delete NAME".into()),
    };
    let entry = keyring::Entry::new(SERVICE, name)?;
    match cmd {
        "set" => {
            let mut value = String::new();
            std::io::stdin().read_to_string(&mut value)?;
            let value = value.trim();
            if value.is_empty() {
                return Err("empty value on stdin".into());
            }
            entry.set_password(value)?;
            eprintln!("stored {name} in the keychain ({SERVICE})");
        }
        "get" => print!("{}", entry.get_password()?),
        "delete" => {
            entry.delete_credential()?;
            eprintln!("deleted {name}");
        }
        _ => return Err("usage: secret set|get|delete NAME".into()),
    }
    Ok(())
}
