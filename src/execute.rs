use std::process::Command;

use crate::{ShellResult, errors::ShellErrors};

pub fn run_command(input: &str) -> ShellResult<()> {
    let parts = parse_command(input);
    if parts.is_none() {
        return Ok(());
    }

    let parts = parts.unwrap();
    println!("command parts: {:?}", parts);

    let cmd = &parts[0];
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

pub fn parse_command(cmd: &str) -> Option<Vec<String>> {
    shlex::split(cmd)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_split_args() {
        let inputs = [
            r#"echo "hello world""#,
            r#"echo 'file with spaces.txt'"#,
            r#"echo hello\ world"#,
        ];

        for input in inputs {
            match shlex::split(input) {
                Some(args) => println!("{args:?}"),
                None => eprintln!("Invalid input: unmatched quote or trailing escape"),
            }
        }
    }
}
