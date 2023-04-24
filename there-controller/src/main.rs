#![forbid(unsafe_code)]

use color_eyre::eyre::Result;
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use there::log::*;
use tracing_subscriber::util::SubscriberInitExt;

mod executor;
mod http_server;
mod keys;

const PORT: u16 = 2345;

#[tokio::main]
#[tracing::instrument]
async fn main() -> Result<()> {
    install_color_eyre()?;
    tracing_subscriber::fmt::SubscriberBuilder::default()
        .with_timer(tracing_subscriber::fmt::time::UtcTime::new(
            time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"),
        ))
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NONE)
        .json()
        .finish()
        .init();

    info!("starting there-controller...");

    info!("ensuring keypair exists...");
    let _client_key = keys::get_or_create_executor_keypair().await?;

    info!("ensuring agent token exists...");
    let token = keys::get_or_create_token().await?;
    println!("* token for agent connections: {token}");
    println!("* agents can bootstrap via:");
    for iface in NetworkInterface::show()? {
        if let Some(addr) = iface.addr.get(0) {
            if addr.ip().is_loopback() {
                continue;
            }
            match addr.ip() {
                std::net::IpAddr::V4(v4) => {
                    let first_octet = v4.octets()[0];
                    let second_octet = v4.octets()[1];
                    if first_octet == 127 // 127.x.x.x
                        || (first_octet == 169 && second_octet == 254) // 169.254.x.x
                        || (first_octet == 172 && (16..=31).contains(&second_octet))
                    // 172.16.x.x - 172.31.x.x
                    {
                        continue;
                    }
                    if v4.is_loopback() {
                        continue;
                    }
                }
                std::net::IpAddr::V6(v6) => {
                    if v6.segments()[0] == 0xfe80 {
                        continue;
                    }
                    if v6.is_loopback() {
                        continue;
                    }
                }
            }

            println!(
                "  - http://{}:{PORT}/api/bootstrap?token={token}",
                addr.ip()
            );
        }
    }

    http_server::run_server(PORT).await
}
