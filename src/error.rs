use std::fmt;

use crate::help::GENERAL_HELP;
use crate::redact::sanitize_text_for_output;

#[derive(Debug)]
pub(crate) struct CliError {
    message: String,
    exit_code: i32,
}

impl CliError {
    pub(crate) fn usage(message: impl Into<String>) -> Self {
        let message = sanitize_text_for_output(&message.into());
        Self {
            message: format!("{message}\n\n{GENERAL_HELP}"),
            exit_code: 2,
        }
    }

    pub(crate) fn message(message: impl Into<String>) -> Self {
        Self {
            message: sanitize_text_for_output(&message.into()),
            exit_code: 1,
        }
    }

    pub(crate) fn exit_code(&self) -> i32 {
        self.exit_code
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}
