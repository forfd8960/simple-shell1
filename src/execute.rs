use std::process::Command;

use crate::{ShellResult, errors::ShellErrors};

pub fn run_command(input: &str) -> ShellResult<()> {
    let child = Command::new(input).spawn();

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
