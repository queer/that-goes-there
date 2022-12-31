//! Simple re-export of logging-related macros.
pub use color_eyre::eyre::eyre;
pub use tracing::{debug, error, info, span, trace, warn};

/// Install color_eyre as the global error handler.
#[tracing::instrument]
pub fn install_color_eyre() -> color_eyre::eyre::Result<()> {
    color_eyre::config::HookBuilder::default()
        .issue_url(concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new"))
        .add_default_filters()
        .add_frame_filter(Box::new(|frames| {
            let filters = &["tokio::", "tracing::", "color_eyre::", "<core::"];

            frames.retain(|frame| {
                !filters.iter().any(|f| {
                    let name = if let Some(name) = frame.name.as_ref() {
                        name.as_str()
                    } else {
                        return true;
                    };

                    name.starts_with(f)
                })
            });
        }))
        .install()?;

    Ok(())
}
