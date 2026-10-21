use std::{
    env,
    fs::{File, OpenOptions},
    path::Path,
    process::{Command, Stdio, exit},
};

use crate::{BuiltInCmd, ShlResult, errors::ShellErrors};

#[derive(Debug, Clone, PartialEq)]
pub struct Redirection {
    pub op: RedirectOp,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RedirectOp {
    Ouput,     // >
    Input,     // <
    Append,    // >>
    ErrOutput, // 2>
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ShlCommand {
    pub name: String,
    pub args: Vec<String>,
    pub rds: Vec<Redirection>,
}

#[derive(Debug)]
pub struct ShlExecutor {
    pub cur_dir: String,
    pub home: String,
    pub history: Vec<ShlCommand>,
}

impl ShlExecutor {
    pub fn new(cur_dir: String, home: String) -> Self {
        Self {
            cur_dir,
            home,
            history: vec![],
        }
    }

    pub fn run_command(&mut self, input: &str) -> ShlResult<()> {
        let shl_cmd = parse_command(input)?;
        println!("parsed shell command: {:?}", shl_cmd);

        self.history.push(shl_cmd.clone());

        let built_cmd = BuiltInCmd::from(shl_cmd.name.as_str());
        if built_cmd != BuiltInCmd::Unknow {
            return self.execute_builtin(&shl_cmd);
        }

        // execute external command
        let mut cmd = Command::new(shl_cmd.name);
        cmd.args(shl_cmd.args);

        for rd in shl_cmd.rds {
            match rd.op {
                RedirectOp::Ouput => {
                    let file = File::create(rd.target)?;
                    cmd.stdout(Stdio::from(file));
                }
                RedirectOp::Append => {
                    let file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(rd.target)?;
                    cmd.stdout(Stdio::from(file));
                }
                RedirectOp::Input => {
                    let file = File::create(rd.target)?;
                    cmd.stdin(Stdio::from(file));
                }
                RedirectOp::ErrOutput => {
                    let file = File::create(rd.target)?;
                    cmd.stderr(Stdio::from(file));
                }
            }
        }

        match cmd.spawn() {
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

    fn execute_builtin(&mut self, cmd: &ShlCommand) -> ShlResult<()> {
        println!("executing builtin command");
        match BuiltInCmd::from(cmd.name.as_str()) {
            BuiltInCmd::Cd => {
                if cmd.args.len() > 1 {
                    return Err(ShellErrors::InvalidArgs(format!("cd only takes 1 arg")));
                }

                let p = cmd.args[0].clone();
                let p = if p.starts_with("~") {
                    p.replacen("~", &self.home, 1)
                } else {
                    p
                };

                env::set_current_dir(Path::new(&p))?;
                self.cur_dir = p;
            }
            BuiltInCmd::Exit => exit(0),
            BuiltInCmd::Export => {
                let exp_args: Vec<&str> = cmd.args[0].split("=").collect();
                if exp_args.len() != 2 {
                    return Err(ShellErrors::InvalidArgs(format!("Echo need 2 args")));
                }
                unsafe { env::set_var(exp_args[0], exp_args[1]) }
            }
            BuiltInCmd::Pwd => {
                println!("{}", self.cur_dir);
            }
            BuiltInCmd::Unknow => {}
        }
        Ok(())
    }
}

pub fn parse_command(cmd: &str) -> ShlResult<ShlCommand> {
    match shlex::split(cmd) {
        Some(parts) => {
            let mut shl_cmd = ShlCommand::default();
            let len = parts.len();

            shl_cmd.name = parts[0].clone();
            let mut i = 1;
            while i < len {
                match parts[i].as_str() {
                    ">" => {
                        if i + 1 >= len {
                            return Err(ShellErrors::InvalidArgs(format!("file required after >")));
                        }

                        shl_cmd.rds.push(Redirection {
                            op: RedirectOp::Ouput,
                            target: parts[i + 1].clone(),
                        });
                        i += 2;
                    }
                    "<" => {
                        if i + 1 >= len {
                            return Err(ShellErrors::InvalidArgs(format!("file required after <")));
                        }

                        shl_cmd.rds.push(Redirection {
                            op: RedirectOp::Input,
                            target: parts[i + 1].clone(),
                        });
                        i += 2;
                    }
                    ">>" => {
                        if i + 1 >= len {
                            return Err(ShellErrors::InvalidArgs(format!(
                                "file required after >>"
                            )));
                        }

                        shl_cmd.rds.push(Redirection {
                            op: RedirectOp::Append,
                            target: parts[i + 1].clone(),
                        });
                        i += 2;
                    }
                    "2>" => {
                        if i + 1 >= len {
                            return Err(ShellErrors::InvalidArgs(format!(
                                "file required after 2>"
                            )));
                        }

                        shl_cmd.rds.push(Redirection {
                            op: RedirectOp::ErrOutput,
                            target: parts[i + 1].clone(),
                        });
                        i += 2;
                    }
                    _ => {
                        shl_cmd.args.push(parts[i].clone());
                        i += 1;
                    }
                }
            }

            Ok(shl_cmd)
        }
        None => Err(ShellErrors::InvalidCommand(format!("no commands"))),
    }
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

    #[test]
    fn test_split_redirections() {
        let inputs = [
            r#"echo "Hello, World!" > hello.txt"#,
            r#"echo "More text" >> hello.txt"#,
            r#"cat < hello.txt"#,
            r#"ls /nonexistent 2> errors.txt"#,
            r#"sort < test.txt > sorted.txt"#,
        ];

        for input in inputs {
            match shlex::split(input) {
                Some(args) => println!("{args:?}"),
                None => eprintln!("Invalid input: unmatched quote or trailing escape"),
            }
        }
    }
}
