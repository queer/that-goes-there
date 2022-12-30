#![forbid(unsafe_code)]

use std::env;
use std::path::Path;

use color_eyre::eyre::Result;
use libthere::log::*;
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<()> {
    install_logger()?;

    let passphrase = env::var("THERE_SSH_PASSPHRASE")?;

    let _client_key = get_or_create_executor_keypair(passphrase).await?;

    let token = get_or_create_token().await?;
    println!("* token for agent connections: {token}");
    println!("* agents can bootstrap via:");
    for iface in NetworkInterface::show()? {
        if let Some(addr) = iface.addr {
            println!("  - {}/api/bootstrap?token={token}", addr.ip());
        }
    }

    Ok(())
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
