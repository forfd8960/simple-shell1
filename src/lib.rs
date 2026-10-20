use crate::errors::ShellErrors;

pub mod errors;
pub mod execute;

pub type ShellResult<T> = Result<T, ShellErrors>;
