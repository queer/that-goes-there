use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::ArgMatches;
use dialoguer::Input;
use regex::Regex;
use thiserror::Error;

pub mod plan;

#[derive(Error, Debug)]
pub enum CommandErrors {
    #[error("Name must be at least 5 characters and contain only lowercase letters, numbers, and dashes (got: {0}).")]
    InvalidNameFormat(String),
    #[error("Prompt interaction failed.")]
    PromptInteractionFailed(
        #[from]
        #[source]
        std::io::Error,
    ),
    #[error("Required user input `{0}` is missing.")]
    RequiredUserInputMissing(String),
    #[error("Argument `{0}` failed validation `{1}`")]
    InputValidationFailure(String, String),
    #[error("Invalid HTTP method `{0}`")]
    InvalidHttpMethod(String),
    #[error("Invalid subcommand `{0}`.")]
    InvalidSubcommand(String),
    #[error("No subcommand provided.")]
    NoSubcommandProvided,
}

pub struct CliContext<'a> {
    pub client: reqwest::Client,
    pub matches: &'a ArgMatches,
}

impl<'a> CliContext<'a> {
    pub fn new(matches: &'a ArgMatches) -> Self {
        Self {
            client: reqwest::Client::new(),
            matches,
        }
    }

    pub fn with_matches(&self, matches: &'a ArgMatches) -> Self {
        Self {
            client: self.client.clone(),
            matches,
        }
    }
}

#[async_trait]
pub trait Command<'a> {
    fn new() -> Self
    where
        Self: Sized;

    async fn run(&self, context: &'a CliContext) -> Result<()>;
}

pub trait Interactive<'a> {
    fn prompt_for_input(&self, message: &'a str) -> Result<String> {
        Input::<String>::new()
            .with_prompt(message)
            .interact()
            .map_err(CommandErrors::PromptInteractionFailed)
            .context("Prompting user input failed.")
    }

    fn prompt_for_input_with_default<S: Into<String>>(
        &self,
        message: &str,
        default: S,
    ) -> Result<String> {
        Input::<String>::new()
            .with_prompt(message)
            .default(default.into())
            .interact()
            .map_err(CommandErrors::PromptInteractionFailed)
            .context("Prompting user input failed.")
    }

    fn prompt_for_input_with_validator<V>(&self, message: &'a str, validator: V) -> Result<String>
    where
        V: FnMut(&String) -> Result<(), CommandErrors>,
    {
        Input::<String>::new()
            .with_prompt(message)
            .validate_with(validator)
            .interact()
            .map_err(CommandErrors::PromptInteractionFailed)
            .context("Prompting user input with validator failed.")
    }

    fn prompt_for_input_with_regex_validation(
        &self,
        message: &'a str,
        regex: &'a Regex,
    ) -> Result<String> {
        self.prompt_for_input_with_validator(message, |input: &String| {
            if regex.is_match(input) {
                Ok(())
            } else {
                Err(CommandErrors::InputValidationFailure(
                    message.to_string(),
                    regex.as_str().to_string(),
                ))
            }
        })
    }

    /// Read argument from the CLI args with a validation function.
    fn read_argument_with_validator<V>(
        &self,
        arg_matches: &'a ArgMatches,
        id: &'a str,
        validator: &mut V,
    ) -> Result<String>
    where
        V: FnMut(&String) -> Result<(), CommandErrors>,
    {
        if let Some(arg) = arg_matches.get_one::<String>(id) {
            validator(arg)?;
            Ok(arg.clone())
        } else {
            Err(CommandErrors::RequiredUserInputMissing(id.into()))?
        }
    }

    /// Read argument from the CLI args with regex validation.
    fn read_argument_with_regex_validation(
        &self,
        arg_matches: &'a ArgMatches,
        id: &'a str,
        regex: &'a Regex,
    ) -> Result<String> {
        self.read_argument_with_validator(arg_matches, id, &mut |input| {
            if regex.is_match(input) {
                Ok(())
            } else {
                Err(CommandErrors::InputValidationFailure(
                    id.into(),
                    regex.as_str().into(),
                ))
            }
        })
    }
}
