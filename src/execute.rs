use std::{
    env::{self},
    fs::{File, OpenOptions},
    mem,
    path::Path,
    process::{Child, Command, Stdio},
};

use crate::{
    BuiltInCmd, ShlResult,
    errors::ShellErrors,
    lexer::{self, Token},
};

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
    pub need_exit: bool,
}

impl ShlExecutor {
    pub fn new(cur_dir: String, home: String) -> Self {
        Self {
            cur_dir,
            home,
            need_exit: false,
        }
    }

    pub fn execute(&mut self, input: &str) -> ShlResult<()> {
        let shl_pipe = parse_pipeline(input)?;
        println!("parsed shell command: {:?}", shl_pipe);
        self.execute_pipeline(&shl_pipe)
    }

    fn execute_pipeline(&mut self, pipe: &Pipeline) -> ShlResult<()> {
        let len = pipe.commands.len();
        match len {
            0 => return Ok(()),
            1 => return self.run_cmd(&pipe.commands[0]),
            _ => return self.run_pipe(pipe),
        }
    }

    fn run_pipe(&mut self, pipe: &Pipeline) -> ShlResult<()> {
        let len = pipe.commands.len();

        let mut children: Vec<Child> = Vec::new();
        for (i, shl_cmd) in pipe.commands.iter().enumerate() {
            let mut cmd = Command::new(shl_cmd.name.clone());
            cmd.args(&shl_cmd.args);

            if i > 0 {
                if let Some(child) = children.last_mut() {
                    let prev_stdout = child
                        .stdout
                        .take()
                        .ok_or_else(|| ShellErrors::Pipe(format!("can not get pre stdout")))?;

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

    fn run_cmd(&mut self, shl_cmd: &ShlCommand) -> ShlResult<()> {
        if let Some(built_in) = BuiltInCmd::from_str(shl_cmd.name.as_str()) {
            return self.execute_builtin(built_in, shl_cmd);
        }

        let mut cmd = Command::new(shl_cmd.name.clone());
        cmd.args(&shl_cmd.args);

        apply_redirections(shl_cmd, &mut cmd)?;

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

    fn execute_builtin(&mut self, built_in: BuiltInCmd, cmd: &ShlCommand) -> ShlResult<()> {
        println!("executing builtin command");

        match built_in {
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
            BuiltInCmd::Exit => self.need_exit = true,
            BuiltInCmd::Export => {
                for arg in &cmd.args {
                    let exp_args: Vec<&str> = arg.split("=").collect();
                    if exp_args.len() != 2 {
                        return Err(ShellErrors::InvalidArgs(format!("invalid env var")));
                    }
                    unsafe { env::set_var(exp_args[0], exp_args[1]) }
                }
            }
            BuiltInCmd::Unset => {
                for arg in &cmd.args {
                    unsafe { env::remove_var(arg) }
                }
            }
            BuiltInCmd::Pwd => {
                println!("{}", self.cur_dir);
            }
        }
        Ok(())
    }
}

pub fn parse_pipeline(input: &str) -> ShlResult<Pipeline> {
    let tokens = lexer::tokenize(input)?;
    let mut commands: Vec<ShlCommand> = Vec::new();
    let mut current: Vec<Token> = Vec::new();

    for token in tokens {
        if token.is_operator("&") {
            return Err(ShellErrors::InvalidCommand(format!(
                "background execution is not supported"
            )));
        }

        if token.is_operator("|") {
            if current.is_empty() {
                return Err(ShellErrors::InvalidCommand(format!(
                    "empty command in pipeline"
                )));
            }

            commands.push(parse_command(mem::take(&mut current))?);
            continue;
        }

        current.push(token);
    }

    if !current.is_empty() {
        commands.push(parse_command(current)?);
    }

    for cmd in commands.iter_mut() {
        cmd.args = expand_command_args(mem::take(&mut cmd.args))?;
    }

    Ok(Pipeline { commands })
}

pub fn parse_command(seg: Vec<Token>) -> ShlResult<ShlCommand> {
    let mut shl_cmd = ShlCommand::default();
    let len = seg.len();

    shl_cmd.name = seg[0].text.clone();
    let mut i = 1;
    while i < len {
        if seg[i].is_operator(">") {
            if i + 1 >= len {
                return Err(ShellErrors::InvalidArgs(format!("file required after >")));
            }

            shl_cmd.rds.push(Redirection {
                op: RedirectOp::Ouput,
                target: seg[i + 1].text.clone(),
            });
            i += 2;
        } else if seg[i].is_operator("<") {
            if i + 1 >= len {
                return Err(ShellErrors::InvalidArgs(format!("file required after <")));
            }

            shl_cmd.rds.push(Redirection {
                op: RedirectOp::Input,
                target: seg[i + 1].text.clone(),
            });
            i += 2;
        } else if seg[i].is_operator(">>") {
            if i + 1 >= len {
                return Err(ShellErrors::InvalidArgs(format!("file required after >>")));
            }

            shl_cmd.rds.push(Redirection {
                op: RedirectOp::Append,
                target: seg[i + 1].text.clone(),
            });
            i += 2;
        } else if seg[i].is_operator("2>") {
            if i + 1 >= len {
                return Err(ShellErrors::InvalidArgs(format!("file required after 2>")));
            }

            shl_cmd.rds.push(Redirection {
                op: RedirectOp::ErrOutput,
                target: seg[i + 1].text.clone(),
            });
            i += 2;
        } else {
            shl_cmd.args.push(seg[i].text.clone());
            i += 1;
        }
    }

    Ok(shl_cmd)
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

fn expand_var(s: &str) -> ShlResult<String> {
    let var_name = s.strip_prefix("$").unwrap();
    let value = env::var(var_name).map_err(|e| ShellErrors::InvalidVar(format!("{e}")))?;

    Ok(value)
}

fn expand_command_args(args: Vec<String>) -> ShlResult<Vec<String>> {
    let mut expand_args = Vec::new();
    for arg in args {
        if arg.starts_with("$") {
            let expand_arg = expand_var(arg.as_str())?;
            expand_args.push(expand_arg);
        } else {
            expand_args.push(arg.clone());
        }
    }
    Ok(expand_args)
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

    #[test]
    fn test_split_export() {
        let inputs = [r#"export RUST_LOG=debug RUST_BACKTRACE=1 PORT=3000"#];

        for input in inputs {
            match shlex::split(input) {
                Some(args) => println!("{args:?}"),
                None => eprintln!("Invalid input: unmatched quote or trailing escape"),
            }
        }
    }
}
