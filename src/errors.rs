use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ShellErrors {
    #[error("{0}")]
    IOError(#[from] io::Error),

    #[error("{0}")]
    InvalidArgs(String),

    #[error("home path is not set")]
    HomeNotSet,
}
