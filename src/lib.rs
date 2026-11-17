use crate::errors::ShellErrors;

pub mod errors;
pub mod execute;
pub mod lexer;

pub type ShlResult<T> = Result<T, ShellErrors>;

#[derive(Debug, PartialEq)]
pub enum BuiltInCmd {
    Cd,
    Exit,
    Pwd,
    Export,
    Unset,
}

impl BuiltInCmd {
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "cd" => Some(Self::Cd),
            "exit" => Some(Self::Exit),
            "export" => Some(Self::Export),
            "unset" => Some(Self::Unset),
            "pwd" => Some(Self::Pwd),
            _ => None,
        }
    }
}
