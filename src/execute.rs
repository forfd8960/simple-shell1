use std::process::Command;

use crate::{ShellResult, errors::ShellErrors};

pub fn run_command(input: &str) -> ShellResult<()> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(());
    }

    let cmd = parts[0];
    let args = &parts[1..];

    let child = Command::new(cmd).args(args).spawn();

    match child {
        Ok(mut child) => match child.wait() {
            Ok(status) => {
                println!("success run: {status}");
            }
            Err(e) => {
                eprintln!("error when wait command: {e}");
                return Err(ShellErrors::IOError(e));
            }
        },
        Err(e) => {
            eprintln!("error when run command: {e}");
            return Err(ShellErrors::IOError(e));
        }
    }

    Ok(())
}
