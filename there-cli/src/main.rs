#![forbid(unsafe_code)]

use anyhow::Result;
use clap::{command, Arg, ArgAction};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::util::SubscriberInitExt;

use crate::commands::Command;

mod commands;
mod executor;

use libthere::log::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Command configuration
    let matches = command!()
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Turn debugging information on. Overrides -q. Can specify up to -vv.")
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .help("Silence all output. Overridden by -v.")
                .action(ArgAction::SetTrue),
        )
        .subcommand(
            command!("plan")
                .about("Manage plans.")
                .subcommand(
                    command!("validate")
                        .about("Validate a plan.")
                        .arg(
                            Arg::new("file")
                                .help("Path to the plan file. No default.")
                                .short('f')
                                .long("file"),
                        ).arg(
                            Arg::new("hosts")
                                .help("Path to the hosts file. No default.")
                                .long("hosts"),
                        ),
                )
                .subcommand(
                    command!("apply")
                        .about("Apply a plan.")
                        .arg(
                            Arg::new("file")
                                .help("Path to the plan file.")
                                .short('f')
                                .long("file"),
                        )
                        .arg(
                            Arg::new("dry")
                                .help("Don't actually apply the plan, just show the changes it will make.")
                                .short('d')
                                .long("dry")
                                .action(ArgAction::SetTrue),
                        )
                        .arg(
                            Arg::new("hosts")
                                .help("Path to the hosts file. No default.")
                                .long("hosts"),
                        )
                        .arg(
                            Arg::new("ssh-key")
                                .help("Path to the SSH key to use for SSH executor.")
                                .short('k')
                                .long("ssh-key"),
                        )
                        .arg(Arg::new("ssh-key-passphrase").help("Path to the SSH key passphrase file.").short('p').long("ssh-key-passphrase"))
                        ,
                )
        )
        .subcommand_required(true)
        .get_matches();

    // Set up logging
    let logging_config = tracing_subscriber::fmt::SubscriberBuilder::default()
        .with_timer(tracing_subscriber::fmt::time::UtcTime::new(
            time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"),
        ))
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NONE)
        .compact();

    let quiet = matches.get_flag("quiet");
    let verbose = matches.get_count("verbose") as usize;
    let logging_config = if quiet && verbose == 0 {
        logging_config.with_max_level(LevelFilter::ERROR)
    } else if verbose > 0 {
        let level = match verbose {
            1 => LevelFilter::WARN,
            2 => LevelFilter::INFO,
            3 => LevelFilter::DEBUG,
            _ => LevelFilter::TRACE,
        };
        logging_config.with_max_level(level)
    } else {
        logging_config.with_max_level(LevelFilter::ERROR)
    };

    let subscriber = logging_config.finish();
    subscriber.init();

    // Run the commands
    if let Some((subcommand, matches)) = matches.subcommand() {
        let ctx = commands::CliContext::new(matches);
        debug!(
            "matched subcommand {} with matches: {:?}",
            &subcommand,
            &matches.ids().map(|id| id.as_str()).collect::<Vec<_>>()
        );
        match subcommand {
            "plan" => commands::plan::PlanCommand::new().run(&ctx).await?,
            _ => return Err(anyhow::anyhow!("Unrecognized subcommand: {}", subcommand)),
        }
    }
    Ok(())
}
