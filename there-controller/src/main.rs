#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use color_eyre::eyre::Result;
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

mod ssh;

#[tokio::main]
async fn main() -> Result<()> {
    let client_key = thrussh_keys::key::KeyPair::generate_ed25519().unwrap();
    let client_pubkey = Arc::new(client_key.clone_public_key());
    let config = thrussh::server::Config {
        keys: vec![client_key],
        connection_timeout: Some(Duration::from_secs(3)),
        auth_rejection_time: Duration::from_secs(3),
        ..Default::default()
    };
    let config = Arc::new(config);
    let sh = ssh::Server {
        client_pubkey,
        clients: Arc::new(Mutex::new(HashMap::new())),
        id: 0,
    };
    println!(
        "* token for agent connections: {}",
        get_or_create_token().await?
    );
    thrussh::server::run(config, "0.0.0.0:2222", sh)
        .await
        .map_err(|e| e.into())
}

async fn get_or_create_token() -> Result<String> {
    let path = Path::new("./agent-token");
    if path.exists() {
        Ok(fs::read_to_string(path).await?)
    } else {
        let mut file = File::create(path).await?;
        let token = generate_token()?;
        file.write_all(token.as_bytes()).await?;
        Ok(token)
    }
}

fn generate_token() -> Result<String> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let token: String = (0..32)
        .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
        .collect();
    Ok(token)
}
