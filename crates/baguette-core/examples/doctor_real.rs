//! Runs the doctor against this computer (no tokens spent).
//! Usage: cargo run -p baguette-core --example doctor_real

use baguette_core::claude::ConnectionMode;

#[tokio::main]
async fn main() {
    for it in baguette_core::doctor::run(ConnectionMode::Subscription, false).await {
        println!("{:<6} {:<28} {:<24} fixes: {:?}", format!("{:?}", it.state), it.name, it.detail, it.fixes);
    }
}
