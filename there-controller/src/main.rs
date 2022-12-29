#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use color_eyre::eyre::Result;
use libthere::log::*;
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

mod ssh;

#[tokio::main]
async fn main() -> Result<()> {
    install_logger()?;

    let passphrase = env::var("THERE_SSH_PASSPHRASE")?;

    let client_key = get_or_create_executor_keypair(passphrase).await?;
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

async fn get_or_create_executor_keypair(passphrase: String) -> Result<thrussh_keys::key::KeyPair> {
    let path = Path::new("./executor-key");
    if path.exists() {
        let key = fs::read_to_string(path).await?;
        thrussh_keys::decode_secret_key(&key, Some(&passphrase)).map_err(|e| e.into())
    } else {
        let mut file = File::create(path).await?;
        // Safety: thrussh_keys always returns Some(...) right now.
        let key = thrussh_keys::key::KeyPair::generate_ed25519().unwrap();
        let mut pem = Vec::new();
        thrussh_keys::encode_pkcs8_pem_encrypted(&key, passphrase.as_bytes(), 1_000_000, &mut pem)?;
        file.write_all(&pem).await?;
        Ok(key)
    }
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
