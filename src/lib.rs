use crate::errors::ShellErrors;

pub mod errors;
pub mod execute;

pub type ShlResult<T> = Result<T, ShellErrors>;

#[derive(Debug, PartialEq)]
pub enum BuiltInCmd {
    Cd,
    Exit,
    Pwd,
    Export,
    Unknow,
}

impl From<&str> for BuiltInCmd {
    fn from(value: &str) -> Self {
        match value {
            "cd" => Self::Cd,
            "exit" => Self::Exit,
            "export" => Self::Export,
            "pwd" => Self::Pwd,
            _ => Self::Unknow,
        }
    }
}
