use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ShellErrors {
    #[error("{0}")]
    IOError(#[from] io::Error),
}
