use std::{
    env,
    fs::{File, OpenOptions},
    path::Path,
    process::{Child, Command, Stdio, exit},
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

#[derive(Debug, Clone)]
pub struct Pipeline {
    pub commands: Vec<ShlCommand>,
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
    pub history: Vec<Pipeline>,
}

impl ShlExecutor {
    pub fn new(cur_dir: String, home: String) -> Self {
        Self {
            cur_dir,
            home,
            history: vec![],
        }
    }

    pub fn execute(&mut self, input: &str) -> ShlResult<()> {
        let shl_pipe = parse_pipeline(input)?;
        println!("parsed shell command: {:?}", shl_pipe);

        self.history.push(shl_pipe.clone());

        self.execute_pipeline(&shl_pipe)
    }

    fn execute_pipeline(&mut self, pipe: &Pipeline) -> ShlResult<()> {
        let len = pipe.commands.len();
        match len {
            0 => return Ok(()),
            1 => return self.run_cmd(&pipe.commands[0]),
            _ => {
                let mut children: Vec<Child> = Vec::new();
                for (i, shl_cmd) in pipe.commands.iter().enumerate() {
                    let mut cmd = Command::new(shl_cmd.name.clone());
                    cmd.args(&shl_cmd.args);

                    if i > 0 {
                        if let Some(child) = children.last_mut() {
                            let prev_stdout = child.stdout.take().ok_or_else(|| {
                                ShellErrors::Pipe(format!("can not get pre stdout"))
                            })?;

                            cmd.stdin(prev_stdout);
                        }
                    } else {
                        cmd.stdin(Stdio::inherit());
                    }

                    if i < len - 1 {
                        cmd.stdout(Stdio::piped());
                    } else {
                        cmd.stdout(Stdio::inherit());
                    }

                    apply_redirections(shl_cmd, &mut cmd)?;

                    let child = cmd.spawn()?;
                    children.push(child);
                }

                for mut child in children {
                    child.wait()?;
                }
                Ok(())
            }
        }
    }

    fn run_cmd(&mut self, shl_cmd: &ShlCommand) -> ShlResult<()> {
        let builtin = BuiltInCmd::from(shl_cmd.name.as_str());
        if builtin != BuiltInCmd::Unknow {
            return self.execute_builtin(shl_cmd);
        }

        let mut cmd = Command::new(shl_cmd.name.clone());
        cmd.args(&shl_cmd.args);

        for rd in &shl_cmd.rds {
            match rd.op {
                RedirectOp::Ouput => {
                    let file = File::create(&rd.target)?;
                    cmd.stdout(Stdio::from(file));
                }
                RedirectOp::Append => {
                    let file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&rd.target)?;
                    cmd.stdout(Stdio::from(file));
                }
                RedirectOp::Input => {
                    let file = File::open(&rd.target)?;
                    cmd.stdin(Stdio::from(file));
                }
                RedirectOp::ErrOutput => {
                    let file = File::create(&rd.target)?;
                    cmd.stderr(Stdio::from(file));
                }
            }
        }

        match cmd.spawn() {
            Ok(mut child) => match child.wait() {
                Ok(status) => {
                    println!("success run: {status}");
                    Ok(())
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

pub fn parse_pipeline(input: &str) -> ShlResult<Pipeline> {
    let cmds: Vec<&str> = input.split("|").collect();
    let mut pipe = Pipeline { commands: vec![] };
    for cmd in cmds {
        pipe.commands.push(parse_command(cmd)?);
    }
    Ok(pipe)
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

fn apply_redirections(shl_cmd: &ShlCommand, cmd: &mut Command) -> ShlResult<()> {
    for rd in &shl_cmd.rds {
        match rd.op {
            RedirectOp::Ouput => {
                let file = File::create(&rd.target)?;
                cmd.stdout(Stdio::from(file));
            }
            RedirectOp::Append => {
                let file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&rd.target)?;
                cmd.stdout(Stdio::from(file));
            }
            RedirectOp::Input => {
                let file = File::open(&rd.target)?;
                cmd.stdin(Stdio::from(file));
            }
            RedirectOp::ErrOutput => {
                let file = File::create(&rd.target)?;
                cmd.stderr(Stdio::from(file));
            }
        }
    }
    Ok(())
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
