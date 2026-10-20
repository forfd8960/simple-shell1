use std::{
    env,
    path::Path,
    process::{Command, exit},
};

use crate::{BuiltInCmd, ShellResult, errors::ShellErrors};

pub fn run_command(input: &str) -> ShellResult<()> {
    let parts = parse_command(input);
    if parts.is_none() {
        return Ok(());
    }

    let parts = parts.unwrap();
    println!("command parts: {:?}", parts);

    let cmd = &parts[0];
    let args = parts[1..].to_vec();

    let built_cmd = BuiltInCmd::from(cmd.as_str());
    if built_cmd != BuiltInCmd::Unknow {
        return execute_builtin(built_cmd, args);
    }

    // execute external command
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

fn execute_builtin(cmd: BuiltInCmd, args: Vec<String>) -> ShellResult<()> {
    println!("executing builtin command");
    match cmd {
        BuiltInCmd::Cd => {
            if args.len() > 1 {
                return Err(ShellErrors::InvalidArgs(format!("cd only takes 1 arg")));
            }

            let p = args[0].clone();
            let p = if p.starts_with("~") {
                let home = match env::var("HOME") {
                    Ok(h) => h,
                    Err(_) => return Err(ShellErrors::HomeNotSet),
                };
                p.replacen("~", home.as_str(), 1)
            } else {
                p
            };

            env::set_current_dir(Path::new(&p))?
        }
        BuiltInCmd::Exit => exit(0),
        BuiltInCmd::Echo => {
            println!("{}", args.join(" "));
        }
        BuiltInCmd::Export => {
            let exp_args: Vec<&str> = args[0].split("=").collect();
            if exp_args.len() != 2 {
                return Err(ShellErrors::InvalidArgs(format!("Echo need 2 args")));
            }
            unsafe { env::set_var(exp_args[0], exp_args[1]) }
        }
        BuiltInCmd::Pwd => {
            let path = env::current_dir()?;
            println!("{}", path.display());
        }
        BuiltInCmd::Unknow => {}
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
