use anyhow::Result;
use clap::{command, Arg, ArgAction};
use tracing_subscriber::filter::LevelFilter;

use crate::commands::Command;

mod commands;

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
                                .help("Path to the plan file. Skips the input prompt if provided.")
                                .short('f')
                                .long("file"),
                        ),
                )
        )
        .subcommand_required(true)
        .get_matches();

    // Set up logging
    let logging_config = tracing_subscriber::fmt::SubscriberBuilder::default()
        .with_timer(tracing_subscriber::fmt::time::UtcTime::new(
            time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"),
        ))
        .with_env_filter(
            "debug,wasmer_compiler_cranelift=warn,cranelift_codegen=info,regalloc=info,clap=info",
        )
        .compact()
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::NONE);

    let quiet = matches.get_flag("quiet");
    let verbose = matches.get_count("verbose") as usize;
    let logging_config = if quiet && verbose == 0 {
        logging_config.with_max_level(LevelFilter::INFO)
    } else if verbose > 0 {
        let level = match verbose {
            1 => LevelFilter::DEBUG,
            _ => LevelFilter::TRACE,
        };
        // 2 is the INFO log level, which is the lowest possible level for
        // verbosity.
        logging_config.with_max_level(level)
    } else {
        logging_config.with_max_level(LevelFilter::INFO)
    };

    let subscriber = logging_config.finish();
    tracing::subscriber::set_global_default(subscriber)?;

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
