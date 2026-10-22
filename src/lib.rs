use crate::errors::ShellErrors;

pub mod errors;
pub mod execute;
pub mod job;

pub type ShlResult<T> = Result<T, ShellErrors>;

#[derive(Debug, PartialEq)]
pub enum BuiltInCmd {
    Cd,
    Exit,
    Pwd,
    Export,
    Unset,
    Jobs,
    Fg,
    Bg,
    Unknow,
}

impl From<&str> for BuiltInCmd {
    fn from(value: &str) -> Self {
        match value {
            "cd" => Self::Cd,
            "exit" => Self::Exit,
            "export" => Self::Export,
            "unset" => Self::Unset,
            "jobs" => Self::Jobs,
            "fg" => Self::Fg,
            "bg" => Self::Bg,
            "pwd" => Self::Pwd,
            _ => Self::Unknow,
        }
    }
}
