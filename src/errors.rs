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

    #[error("invalid command: {0}")]
    InvalidCommand(String),

    #[error("pipe error: {0}")]
    Pipe(String),

    #[error("invalid var: {0}")]
    InvalidVar(String),

    #[error("tty error: {0}")]
    TTYError(String),

    #[error("signal error: {0}")]
    SigError(String),

    #[error("nix error: {0}")]
    NixError(#[from] nix::Error),

    #[error("job error: {0}")]
    JobError(String),
}
