use std::path::Path;

use color_eyre::eyre::Result;
use libthere::log::*;
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

#[tracing::instrument]
pub fn passphrase() -> Result<String> {
    std::env::var("THERE_SSH_PASSPHRASE").map_err(|e| e.into())
}

#[tracing::instrument]
pub async fn get_or_create_executor_keypair() -> Result<thrussh_keys::key::KeyPair> {
    let path = Path::new("./there-controller-executor-key");
    if path.exists() {
        let key = fs::read_to_string(path).await?;
        thrussh_keys::decode_secret_key(&key, Some(&passphrase()?)).map_err(|e| e.into())
    } else {
        let mut file = File::create(path).await?;
        // Safety: thrussh_keys always returns Some(...) right now.
        let key = thrussh_keys::key::KeyPair::generate_ed25519().unwrap();
        let mut pem = Vec::new();
        debug!("encrypting key with 10_000 rounds...");
        thrussh_keys::encode_pkcs8_pem_encrypted(&key, passphrase()?.as_bytes(), 10_000, &mut pem)?;
        file.write_all(&pem).await?;
        Ok(key)
    }
}

#[tracing::instrument]
pub async fn get_or_create_token() -> Result<String> {
    let path = Path::new("./there-controller-agent-token");
    if path.exists() {
        Ok(fs::read_to_string(path).await?)
    } else {
        let mut file = File::create(path).await?;
        let token = generate_token()?;
        file.write_all(token.as_bytes()).await?;
        Ok(token)
    }
}

#[tracing::instrument]
fn generate_token() -> Result<String> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let token: String = (0..32)
        .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
        .collect();
    Ok(token)
}
