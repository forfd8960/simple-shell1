# Learn job control with `Command` and `nix`

This guide adds job control to the current Rush shell using `std::process::Command` to start commands and `nix` for Unix signals, waiting, and terminal ownership. All code uses those public APIs: there is no direct `libc` dependency, import, or call. `nix` itself uses operating-system bindings internally.

Start with **one external process per job**. Learn and test these commands before extending the implementation to pipelines:

```text
rush[0]> sleep 30 &
[1] 12345
rush[1]> jobs
[1] Running  sleep 30 &
rush[2]> fg %1
# Press Ctrl+Z here to stop sleep and return to Rush.
rush[3]> bg %1
[1] Running  sleep 30 &
rush[4]> fg %1
# Press Ctrl+C here to terminate sleep and return to Rush.
```

This is a Unix-only learning version for macOS/Linux. Run it in a real terminal. It deliberately requires interactive foreground startup, uses explicit IDs for `fg`/`bg`, and postpones pipeline job control and asynchronous notifications. The snippets are proposed changes; writing this guide does not modify the shell source.

## 1. Understand the three IDs

| Name | Meaning | Example |
| --- | --- | --- |
| Job ID | Number assigned by your shell | `1`, entered as `%1` |
| PID | Operating-system ID of one process | `12345` |
| PGID | Operating-system ID of a process group | `12345` for this one-process job |

Each new job gets a new process group. The terminal sends Ctrl+C and Ctrl+Z to its **foreground process group**. Rush hands the terminal to a foreground job, waits, and takes it back when the job exits or stops.

`bg` sends `SIGCONT` while Rush keeps the terminal. `fg` hands over the terminal, sends `SIGCONT` if necessary, and waits. Sending `SIGCONT` alone does not make a job foreground.

Rust's [`CommandExt::process_group`](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.process_group) creates or joins a process group. The value `0` means “use the new child's PID as its PGID.”

## 2. Add the dependency

Add this to `Cargo.toml`, keeping your existing dependencies:

```toml
[target.'cfg(unix)'.dependencies]
nix = { version = "0.31", features = ["process", "signal", "term"] }
```

The examples target the `nix` 0.31 API. In particular, terminal functions accept a borrowed file such as `&tty`, rather than a bare integer descriptor. See [`tcsetpgrp`](https://docs.rs/nix/latest/nix/unistd/fn.tcsetpgrp.html).

## 3. Add `src/job.rs`

Use a small `JobManager` that owns the job table and terminal state. For now, each job stores one `Child` plus its PGID and state. `BTreeMap` prints IDs in order.

**One waiting rule:** after registering a child, only this manager calls `waitpid`. Do not also call `Child::wait()` or `try_wait()` for that child. Two waiters can consume each other's exit information.

The complete module below can be copied into `src/job.rs`:

```rust
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{self, IsTerminal},
    os::unix::process::CommandExt,
    process::{Child, Command},
};

use nix::{
    errno::Errno,
    sys::{
        signal::{killpg, sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal},
        termios::{tcgetattr, tcsetattr, SetArg, Termios},
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::{getpgrp, getpid, setpgid, tcgetpgrp, tcsetpgrp, Pid},
};

const SHELL_SIGNALS: [Signal; 5] = [
    Signal::SIGINT,
    Signal::SIGQUIT,
    Signal::SIGTSTP,
    Signal::SIGTTIN,
    Signal::SIGTTOU,
];

fn io_error(error: Errno) -> io::Error {
    io::Error::from_raw_os_error(error as i32)
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Running,
    Stopped,
    Done,
}

pub struct Job {
    pub id: usize,
    pub command: String,
    pub pgid: Pid,
    pub state: JobState,
    child: Child,
    terminal_modes: Option<Termios>,
}

pub struct JobManager {
    jobs: BTreeMap<usize, Job>,
    next_id: usize,
    tty: File,
    shell_pgid: Pid,
    shell_modes: Termios,
    old_signals: Vec<(Signal, SigAction)>,
}

impl JobManager {
    pub fn new() -> io::Result<Self> {
        if !io::stdin().is_terminal() {
            return Err(invalid("this learning version requires an interactive terminal"));
        }
        let tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
        if tcgetpgrp(&tty).map_err(io_error)? != getpgrp() {
            return Err(invalid("start Rush in the foreground"));
        }
        let shell_modes = tcgetattr(&tty).map_err(io_error)?;
        let mut manager = Self {
            jobs: BTreeMap::new(),
            next_id: 1,
            tty,
            shell_pgid: getpgrp(),
            shell_modes,
            old_signals: Vec::new(),
        };

        let ignore = SigAction::new(
            SigHandler::SigIgn, SaFlags::empty(), SigSet::empty(),
        );
        for signal in SHELL_SIGNALS {
            // SAFETY: install the fixed ignore disposition, not a Rust callback.
            let old = unsafe { sigaction(signal, &ignore) }.map_err(io_error)?;
            manager.old_signals.push((signal, old));
        }
        // Rust may ignore SIGPIPE; do not change it in the parent here.
        // Leave SIGCHLD waitable: do not install SIG_IGN or SA_NOCLDWAIT.
        if getpgrp() != getpid() {
            setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(io_error)?;
        }
        manager.shell_pgid = getpgrp();
        tcsetpgrp(&manager.tty, manager.shell_pgid).map_err(io_error)?;
        Ok(manager)
    }

    pub fn launch(
        &mut self,
        mut command: Command,
        text: &str,
        background: bool,
    ) -> io::Result<()> {
        command.process_group(0);
        let defaults = SigAction::new(
            SigHandler::SigDfl, SaFlags::SA_RESTART, SigSet::empty(),
        );
        // SAFETY: this hook only resets fixed signal dispositions. It does not
        // allocate, print, lock, or access the shell's mutable data after fork.
        // Keep this learning shell single-threaded.
        unsafe {
            command.pre_exec(move || {
                for signal in SHELL_SIGNALS.into_iter().chain([
                    Signal::SIGPIPE, Signal::SIGCHLD,
                ]) {
                    sigaction(signal, &defaults).map_err(io_error)?;
                }
                Ok(())
            });
        }
        
        let child = command.spawn()?;
        let pgid = Pid::from_raw(child.id() as i32);
        
        let id = self.next_id;
        self.next_id += 1;
        
        self.jobs.insert(id, Job {
            id,
            command: text.to_owned(),
            pgid,
            state: JobState::Running,
            child,
            terminal_modes: None,
        });
        if background {
            println!("[{id}] {pgid}");
            Ok(())
        } else {
            self.wait_foreground(id)
        }
    }

    fn active_job(&self, id: usize) -> io::Result<&Job> {
        self.jobs.get(&id).filter(|job| job.state != JobState::Done)
            .ok_or_else(|| invalid("no such active job"))
    }

    fn update(job: &mut Job, status: WaitStatus) {
        match status {
            WaitStatus::Exited(_, _) | WaitStatus::Signaled(_, _, _) => {
                job.state = JobState::Done;
            }
            WaitStatus::Stopped(_, _) => job.state = JobState::Stopped,
            WaitStatus::Continued(_) => job.state = JobState::Running,
            _ => {}
        }
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        let flags = WaitPidFlag::WNOHANG
            | WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED;
        for job in self.jobs.values_mut() {
            while job.state != JobState::Done {
                let pid = Pid::from_raw(job.child.id() as i32);
                match waitpid(pid, Some(flags)) {
                    Ok(WaitStatus::StillAlive) => break,
                    Ok(status) => Self::update(job, status),
                    Err(Errno::EINTR) => continue,
                    Err(error) => return Err(io_error(error)),
                }
            }
        }
        Ok(())
    }

    pub fn list(&mut self) -> io::Result<()> {
        self.refresh()?;
        for job in self.jobs.values() {
            println!("[{}] {:?}\t{}", job.id, job.state, job.command);
        }
        // Display completed jobs once, then forget them.
        self.jobs.retain(|_, job| job.state != JobState::Done);
        Ok(())
    }

    pub fn background(&mut self, id: usize) -> io::Result<()> {
        self.refresh()?;
        let pgid = self.active_job(id)?.pgid;
        killpg(pgid, Signal::SIGCONT).map_err(io_error)?;
        let job = self.jobs.get_mut(&id).unwrap();
        job.state = JobState::Running;
        println!("[{id}] Running\t{}", job.command);
        Ok(())
    }

    pub fn foreground(&mut self, id: usize) -> io::Result<()> {
        self.refresh()?;
        self.active_job(id)?;
        self.wait_foreground(id)
    }

    fn wait_foreground(&mut self, id: usize) -> io::Result<()> {
        let pgid = self.active_job(id)?.pgid;
        // Always restore terminal ownership/settings, even if waiting fails.
        let result = (|| {
            tcsetpgrp(&self.tty, pgid).map_err(io_error)?;
            let job = self.jobs.get_mut(&id).unwrap();
            if let Some(modes) = &job.terminal_modes {
                tcsetattr(&self.tty, SetArg::TCSADRAIN, modes).map_err(io_error)?;
            }
            if job.state == JobState::Stopped {
                killpg(pgid, Signal::SIGCONT).map_err(io_error)?;
                job.state = JobState::Running;
            }
            let flags = WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED;
            while job.state == JobState::Running {
                // A negative PID waits for children in this process group.
                match waitpid(Pid::from_raw(-pgid.as_raw()), Some(flags)) {
                    Ok(status) => Self::update(job, status),
                    Err(Errno::EINTR) => continue,
                    Err(error) => return Err(io_error(error)),
                }
            }
            if job.state == JobState::Stopped {
                job.terminal_modes = Some(tcgetattr(&self.tty).map_err(io_error)?);
            }
            Ok(())
        })();
        let restored = self.restore_terminal();
        result?;
        restored?;
        if self.jobs[&id].state == JobState::Done {
            self.jobs.remove(&id);
        } else {
            println!("[{id}] Stopped\t{}", self.jobs[&id].command);
        }
        Ok(())
    }

    fn restore_terminal(&self) -> io::Result<()> {
        tcsetpgrp(&self.tty, self.shell_pgid).map_err(io_error)?;
        tcsetattr(&self.tty, SetArg::TCSADRAIN, &self.shell_modes).map_err(io_error)
    }
}

impl Drop for JobManager {
    fn drop(&mut self) {
        let _ = self.refresh();
        for job in self.jobs.values().filter(|job| job.state != JobState::Done) {
            let _ = killpg(job.pgid, Signal::SIGHUP);
            let _ = killpg(job.pgid, Signal::SIGCONT);
        }
        // Reap promptly exiting children, without hanging on jobs ignoring HUP.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
        loop {
            let _ = self.refresh();
            if self.jobs.values().all(|job| job.state == JobState::Done)
                || std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let _ = self.restore_terminal();
        for (signal, old) in &self.old_signals {
            // SAFETY: restore the exact saved disposition.
            let _ = unsafe { sigaction(*signal, old) };
        }
    }
}
```

Why is there still `unsafe`? Changing signal dispositions and running a hook between fork and exec require care even through `nix`. Here the hook only installs default dispositions. Keep allocations, error messages, and normal Rust application logic out of it. See Rust's [`pre_exec` safety contract](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.pre_exec) and the [`nix` signal module](https://docs.rs/nix/latest/nix/sys/signal/).

Why reset child signals? Rush ignores signals that should not stop or terminate the shell itself. A command needs their normal behavior, so Ctrl+C terminates it and Ctrl+Z stops it. Ignored dispositions can survive exec.

Why use `waitpid`? It reports stops and continuations as well as exits. `WUNTRACED` requests stop events, `WCONTINUED` requests resume events, and `WNOHANG` makes a refresh return immediately when nothing changed. See [`WaitStatus`](https://docs.rs/nix/latest/nix/sys/wait/enum.WaitStatus.html) and [`waitpid`](https://docs.rs/nix/latest/nix/sys/wait/fn.waitpid.html). An unexpected `ECHILD` is an error here: check that no other code reaped the registered child.

The terminal-settings code saves Rush's settings and a stopped job's settings separately. It restores each when that owner receives the terminal again. This matters for commands that disable echo or change input modes. See [`nix::sys::termios`](https://docs.rs/nix/latest/nix/sys/termios/).

## 4. Connect the module to your executor

In `src/lib.rs`, export the module, add builtin variants, and add their string mappings:

```rust
pub mod job;

// Add to BuiltInCmd:
// Jobs,
// Fg,
// Bg,

// Add to BuiltInCmd::from's match:
// "jobs" => Self::Jobs,
// "fg" => Self::Fg,
// "bg" => Self::Bg,
```

Add two fields to `ShlExecutor`:

```rust
pub jobs: crate::job::JobManager,
pub exit_requested: bool,
```

Because constructing the manager can fail, change your constructor to return `ShlResult<Self>`:

```rust
pub fn new(cur_dir: String, home: String) -> ShlResult<Self> {
    Ok(Self {
        cur_dir,
        home,
        history: vec![],
        jobs: crate::job::JobManager::new()?,
        exit_requested: false,
    })
}
```

Update the construction in `main.rs` to `ShlExecutor::new(cur_dir, home)?`. Remove `#[derive(Debug)]` from `ShlExecutor` for this version: the manager contains signal and terminal state and does not derive `Debug`.

Add this helper in `execute.rs` for explicit IDs:

```rust
fn job_id(args: &[String]) -> ShlResult<usize> {
    if args.len() != 1 {
        return Err(ShellErrors::InvalidArgs("usage: fg/bg %job_id".into()));
    }
    args[0].strip_prefix('%').unwrap_or(&args[0])
        .parse::<usize>()
        .ok().filter(|id| *id > 0)
        .ok_or_else(|| ShellErrors::InvalidArgs("invalid job ID".into()))
}
```

Add these arms to `execute_builtin()`:

```rust
BuiltInCmd::Jobs => {
    if !cmd.args.is_empty() {
        return Err(ShellErrors::InvalidArgs("usage: jobs".into()));
    }
    self.jobs.list()?;
}
BuiltInCmd::Fg => self.jobs.foreground(job_id(&cmd.args)?)?,
BuiltInCmd::Bg => self.jobs.background(job_id(&cmd.args)?)?,
BuiltInCmd::Exit => self.exit_requested = true,
```

Replace the old `Exit => exit(0)` arm rather than keeping both. Remove the unused `exit` import. Returning to `main` allows `JobManager::drop()` to restore terminal settings and notify remaining jobs.

## 5. Recognize a final unquoted `&`

Add `pub background: bool` to `Pipeline`. Keep operator recognition separate from quote removal. The following helper finds only unquoted, unescaped `&` and `|`; it leaves redirection parsing to your existing code:

```rust
fn simple_job_input(input: &str) -> ShlResult<(&str, bool)> {
    let input = input.trim_end();
    let mut quote = None;
    let mut escaped = false;
    let mut background = false;
    let mut chars = input.char_indices().peekable();

    while let Some((index, c)) = chars.next() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && quote != Some('\'') {
            escaped = quote.is_none() || chars.peek().is_some_and(|(_, next)| {
                ['"', '\\', '$', '`', '\n'].contains(next)
            });
            continue;
        }
        if let Some(q) = quote {
            if c == q { quote = None; }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '|' => return Err(ShellErrors::InvalidCommand(
                "pipeline job control is a later step in this guide".into(),
            )),
            '&' if index + 1 == input.len() => background = true,
            '&' => return Err(ShellErrors::InvalidCommand(
                "only one final & is supported".into(),
            )),
            _ => {}
        }
    }
    if quote.is_some() || escaped {
        return Err(ShellErrors::InvalidCommand("unclosed quote or escape".into()));
    }
    let command = if background { &input[..input.len() - 1] } else { input };
    if command.trim().is_empty() {
        return Err(ShellErrors::InvalidCommand("missing command".into()));
    }
    Ok((command, background))
}
```

For this learning stage, replace `parse_pipeline()` with:

```rust
pub fn parse_pipeline(input: &str) -> ShlResult<Pipeline> {
    let (command, background) = simple_job_input(input)?;
    Ok(Pipeline {
        commands: vec![parse_command(command)?],
        background,
    })
}
```

This handles `sleep 30&`, `echo '&'`, and `echo a\&b`. It explicitly rejects pipeline execution for now, so quoted `|` is safe while the old `input.split("|")` approach is no longer used. Inline shell comments and compound commands are outside this helper's scope. Your existing redirection parser still requires spaces around operators and does not preserve quoted redirection operators; improve that separately with typed tokens later.

Also guard `parts[0]` in the existing `parse_command()`:

```rust
// Immediately after `Some(parts) => {`:
if parts.is_empty() {
    return Err(ShellErrors::InvalidCommand("missing command".into()));
}
```

## 6. Hand external commands to the manager

Replace `execute_pipeline()` with this temporary one-command version:

```rust
fn execute_pipeline(&mut self, pipe: &Pipeline) -> ShlResult<()> {
    if pipe.commands.len() != 1 {
        return Err(ShellErrors::InvalidCommand(
            "this job-control lesson supports one command per job".into(),
        ));
    }
    let shl_cmd = &pipe.commands[0];
    let builtin = BuiltInCmd::from(shl_cmd.name.as_str());
    if builtin != BuiltInCmd::Unknow {
        if pipe.background || !shl_cmd.rds.is_empty() {
            return Err(ShellErrors::InvalidArgs(
                "background execution/redirection for builtins is not supported yet".into(),
            ));
        }
        return self.execute_builtin(shl_cmd);
    }

    let mut command = Command::new(&shl_cmd.name);
    command.args(&shl_cmd.args);
    apply_redirections(shl_cmd, &mut command)?;
    self.jobs.launch(command, &shl_cmd.name, pipe.background)?;
    Ok(())
}
```

For useful job listings, pass the original command text instead of just `shl_cmd.name`: add `text: &str` to `execute_pipeline()` and pass `input` from `execute()`, then use `self.jobs.launch(command, text, pipe.background)?`.

Remove the old `run_cmd()` external spawning/waiting path and the old `Vec<Child>` pipeline loop. Keep `apply_redirections()`—it still configures the `Command` before launch. The manager owns every registered child's waiting now. Retain the imports that redirection setup uses; remove unused `Child` and `exit` imports from `execute.rs`.

## 7. Adjust the REPL

Keep your current prompt and input code, with these changes:

```rust
// Constructor now returns a Result.
let mut executor = ShlExecutor::new(cur_dir, home)?;
let mut cmd_count = 0;

loop {
    executor.jobs.refresh()?;
    print!("rush[{cmd_count}]> ");
    stdout().flush()?;

    let mut input = String::new();
    if stdin().read_line(&mut input)? == 0 {
        break; // EOF, including Ctrl+D on an empty terminal input line.
    }
    let input = input.trim();
    if input.is_empty() { continue; }

    if let Err(error) = executor.execute(input) {
        eprintln!("rush: {error}");
    }
    if executor.exit_requested { break; }
    cmd_count += 1;
}
```

Remove the old `if input == "exit" { break; }` special case so the builtin handles exit consistently. Reaching the end of `main` drops the executor and its job manager.

The refresh-before-prompt approach is intentionally simple: a background child that exits while Rush waits for input is reaped on the next command. It will not print a completion message immediately. `jobs` displays completed jobs once. No signal handler or event loop is needed for this first version.

## 8. Try these checks in a terminal

Run `cargo check` first, then `cargo run` in a terminal.

| Try | Expected behavior |
| --- | --- |
| `sleep 30 &`, then `jobs` | Immediate prompt; job is Running. |
| `fg %1`, then Ctrl+Z | Sleep stops; Rush regains the terminal and prints Stopped. |
| `bg %1` | Sleep resumes; prompt remains available. |
| `fg %1`, then Ctrl+C | Sleep terminates; Rush survives. |
| `sleep 1 &`, wait, then `jobs` | Done appears once, then disappears on the next `jobs`. |
| `fg %999`, `bg nope`, `jobs extra` | Error message; the shell keeps running. |
| `echo '&'` | Prints a literal ampersand; not background execution. |
| `sleep 30 &`, then `exit` | Shutdown sends SIGHUP and SIGCONT to remaining active groups. |

Use the ID actually printed by Rush; foreground jobs also consume IDs. Ctrl+C at the prompt is ignored in this version. If you later add a raw-mode line editor, restore normal terminal mode before foreground execution so Ctrl+C and Ctrl+Z remain terminal-generated signals.

On the macOS machine used during the earlier investigation, the native `cat` exited with `Interrupted system call` after being resumed from a background terminal read; the system Bash reproduced that behavior too. Begin with `sleep` for stop/resume practice. A reader that retries interrupted reads is a better automated test of input after `fg`.

## 9. What to learn next

The `Command` launcher above has a timing limitation: a newly spawned foreground program can run before the parent calls `tcsetpgrp()`. An interactive program may attempt an early terminal read and stop with SIGTTIN. If that happens, `fg %N` resumes it after handoff. This simple implementation does not eliminate all launch races.

Do not put a blocking startup barrier or `SIGSTOP` inside `pre_exec()`: `Command::spawn()` can wait for the child's exec handshake, so the parent may never return to release the child. The hook above only resets signals and returns.

After the one-process version makes sense, add pipelines by storing a vector of process states per job, placing all pipeline members in one PGID, and deriving the job's aggregate state. Spawn every pipeline member before waiting, and keep unused pipe descriptors closed. A fast first process can exit before later members join its group; reliable grouping and terminal handoff require a more careful launch protocol. Simply setting `process_group(first_pid)` on later commands does not solve that race.

Then add prompt-time SIGCHLD notifications and robust launch synchronization as separate lessons. Keep all job-table changes and reaping in normal shell control flow. This progression keeps the first version small enough to understand while making its boundaries explicit.
