#![forbid(unsafe_code)]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::Duration;

use color_eyre::eyre;
use color_eyre::eyre::Result;
use libthere::log::*;

fn main() -> Result<()> {
    let controller_dsn = std::env::var("THERE_CONTROLLER_BOOTSTRAP_DSN")?;
    info!("connecting to: {controller_dsn}");

    loop {
        let controller_key = reqwest::blocking::get(controller_dsn.clone())?.text()?;
        // Read ~/.ssh/authorized_keys, check if controller_key is in it, and add it if it's not.
        let ssh_key_path = directories::UserDirs::new()
            .ok_or_else(|| eyre::eyre!("could not get user directories"))?
            .home_dir()
            .join(".ssh/authorized_keys");

        let maybe_existing_key = fs::read_to_string(&ssh_key_path)?;
        let maybe_existing_key = maybe_existing_key
            .lines()
            .find(|line| line == &controller_key);
        if maybe_existing_key.is_none() {
            // Append ssh key to the end of file.
            let mut file = OpenOptions::new()
                .write(true)
                .append(true)
                .open(&ssh_key_path)
                .unwrap();

            if let Err(e) = writeln!(file, "{controller_key}") {
                panic!("{}", e);
            }
        }

        // Poll every 5 minutes
        std::thread::sleep(Duration::from_secs(60 * 5));
    }

    #[allow(unreachable_code)]
    Ok(())
}
