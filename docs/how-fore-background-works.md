# How background and foreground jobs work

This explanation follows `background()`, `foreground()`, and `wait_foreground()` in [job-implement.md](job-implement.md). That learning implementation manages one external process per job using `Command` and `nix`.

## First, separate process state from terminal ownership

A job can be running in either the foreground or the background. `JobState::Running` only says it is not recorded as stopped or finished; it does not say who owns the terminal.

| Situation | Terminal foreground group | What Rush does |
| --- | --- | --- |
| Job running in foreground | Job's process group | Waits for the job to stop or exit |
| Job running in background | Rush's process group | Accepts more commands |
| Job stopped, prompt displayed | Rush's process group | Keeps the job available for `fg` or `bg` |

The terminal sends Ctrl+C and Ctrl+Z to its foreground process group. A background process that attempts to read its controlling terminal normally receives SIGTTIN and stops. Background output can still appear on the terminal by default; being in the background does not redirect output.

Both `fg` and `bg` operate on an existing job. They do not spawn a new process or change its PID or PGID.

## How `background()` works

```rust
pub fn background(&mut self, id: usize) -> io::Result<()> {
    self.refresh()?;
    let pgid = self.active_job(id)?.pgid;
    killpg(pgid, Signal::SIGCONT).map_err(io_error)?;
    let job = self.jobs.get_mut(&id).unwrap();
    job.state = JobState::Running;
    println!("[{id}] Running\t{}", job.command);
    Ok(())
}
```

### 1. Refresh the recorded job states

```rust
self.refresh()?;
```

A job may have stopped or finished since the last prompt. `refresh()` collects pending child status changes with nonblocking `waitpid()` calls and updates the job table.

### 2. Find the selected job's process group

```rust
let pgid = self.active_job(id)?.pgid;
```

`id` is the shell job ID, such as `1` from `bg %1`. `active_job()` checks that this job exists and is not Done, then returns a reference to it. `?` returns an error if the check fails.

The PGID is the operating-system identifier used to signal the whole job.

### 3. Resume the process group

```rust
killpg(pgid, Signal::SIGCONT).map_err(io_error)?;
```

`killpg()` sends a signal to a process group. Despite its name, it does not always terminate processes: the signal determines the action. `SIGCONT` resumes stopped processes. Sending it to a running job does not restart the program from the beginning.

`map_err(io_error)` converts the `nix` error into `std::io::Error`. If signalling fails, `?` returns before the code marks the job Running.

### 4. Update the table and return

```rust
let job = self.jobs.get_mut(&id).unwrap();
job.state = JobState::Running;
println!("[{id}] Running\t{}", job.command);
Ok(())
```

`get_mut()` obtains a mutable reference so the stored state can change. The earlier lookup established that the job exists, and nothing removes it between the lookups in this single-threaded implementation, so `unwrap()` is valid here.

The code records the successful resume request immediately. A later refresh can discover that the job stopped again or exited.

There is no `tcsetpgrp()` call and no blocking wait. Rush keeps the terminal and returns to its prompt. If a resumed background job immediately tries to read the terminal, it may stop again with SIGTTIN.

## How `foreground()` works

```rust
pub fn foreground(&mut self, id: usize) -> io::Result<()> {
    self.refresh()?;
    self.active_job(id)?;
    self.wait_foreground(id)
}
```

This public method handles the `fg %N` builtin:

1. Refresh the job states.
2. Check that the requested job still exists and is active.
3. Delegate terminal handoff and waiting to `wait_foreground()`.

The final expression returns the result of `wait_foreground()` directly. The helper is also used by `launch()` when starting a new foreground job, so both paths share the same waiting and cleanup logic.

## How `wait_foreground()` works

### 1. Look up the process group

```rust
let pgid = self.active_job(id)?.pgid;
```

The helper needs the PGID to select which group receives the terminal and which children it waits for.

### 2. Start an operation whose errors can be saved

```rust
let result = (|| {
    // Hand over the terminal, resume if needed, and wait.
    Ok(())
})();
```

`|| { ... }` creates a closure, and the final `()` calls it immediately. Its return value is stored in `result`.

This structure matters for cleanup: a `?` inside this closure returns from the closure, allowing the outer function to continue and attempt terminal restoration. Without this structure, an early `?` could return from `wait_foreground()` while the job still owns the terminal.

### 3. Give the job terminal ownership

```rust
tcsetpgrp(&self.tty, pgid).map_err(io_error)?;
```

This selects the job's group as the terminal's foreground process group. Its processes can now read terminal input and receive terminal-generated signals such as Ctrl+C and Ctrl+Z.

This is the step that makes the job foreground. Waiting alone would not transfer terminal ownership.

### 4. Restore the job's saved terminal settings

```rust
let job = self.jobs.get_mut(&id).unwrap();
if let Some(modes) = &job.terminal_modes {
    tcsetattr(&self.tty, SetArg::TCSADRAIN, modes).map_err(io_error)?;
}
```

An interactive program may change terminal settings—for example, disable character echo. If it was previously stopped, Rush saved those settings. Restoring them lets the program continue with the terminal configuration it expects.

`TCSADRAIN` waits for queued terminal output to be transmitted before applying the settings. A newly launched job has no saved settings, so this block is skipped.

### 5. Resume the job if it was stopped

```rust
if job.state == JobState::Stopped {
    killpg(pgid, Signal::SIGCONT).map_err(io_error)?;
    job.state = JobState::Running;
}
```

A running background job already runs, so it only needs the terminal handoff and waiting. A stopped job also needs `SIGCONT`.

The order is important: give the job the terminal before resuming it. Otherwise it might immediately attempt a terminal read while still in the background and stop again.

### 6. Wait for status changes

```rust
let flags = WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED;
while job.state == JobState::Running {
    match waitpid(Pid::from_raw(-pgid.as_raw()), Some(flags)) {
        Ok(status) => Self::update(job, status),
        Err(Errno::EINTR) => continue,
        Err(error) => return Err(io_error(error)),
    }
}
```

The negative value passed to `waitpid()` selects child processes in this process group. This guide currently has one child per group.

- `WUNTRACED` requests notification when a child stops.
- `WCONTINUED` requests notification when a stopped child resumes.
- There is no `WNOHANG`, so the call can block while waiting for a change.

`Self::update()` translates the reported status into the stored state:

| Status | Recorded state |
| --- | --- |
| Exited normally | Done |
| Terminated by a signal | Done |
| Stopped, such as by Ctrl+Z | Stopped |
| Continued | Running |

The loop keeps waiting while the recorded state is Running. It ends once the job stops or finishes. `EINTR` means the wait was interrupted by a signal, so the loop retries. Other errors leave the closure and proceed to terminal cleanup.

While Rush waits, the job can execute and interact with the user. Rush is not accepting new commands during this foreground wait.

### 7. Save terminal settings if the job stopped

```rust
if job.state == JobState::Stopped {
    job.terminal_modes = Some(tcgetattr(&self.tty).map_err(io_error)?);
}
```

Capture the terminal configuration left by the stopped program before restoring Rush's settings. A later `fg` restores this saved configuration in step 4.

### 8. Always attempt to restore Rush's terminal

```rust
let restored = self.restore_terminal();
result?;
restored?;
```

Notice that `restore_terminal()` runs before either `?`. Even when the closure returned an error, terminal restoration is still attempted.

The restoration helper does two things:

```rust
tcsetpgrp(&self.tty, self.shell_pgid).map_err(io_error)?;
tcsetattr(&self.tty, SetArg::TCSADRAIN, &self.shell_modes).map_err(io_error)
```

First Rush becomes the terminal's foreground group again. Then it restores its saved settings so the prompt has normal input and echo behavior.

If both the operation and restoration fail, this code returns the operation's error first. If only restoration fails, it returns that restoration error.

### 9. Remove finished jobs or retain stopped jobs

```rust
if self.jobs[&id].state == JobState::Done {
    self.jobs.remove(&id);
} else {
    println!("[{id}] Stopped\t{}", self.jobs[&id].command);
}
Ok(())
```

A completed foreground job has been reaped and can be removed. A stopped job must stay in the table so `jobs`, `bg`, and `fg` can find it later.

The function then returns to the REPL, which prints the next prompt.

## Follow one job through the commands

```text
sleep 30 &     Start a new job; Rush keeps the terminal.
fg %1          Give the existing job the terminal and wait.
Ctrl+Z         Terminal sends SIGTSTP; job stops.
               Rush notices, takes the terminal back, and prints a prompt.
bg %1          Send SIGCONT; job runs while Rush keeps the terminal.
fg %1          Give the running job the terminal and wait again.
Ctrl+C         Terminal sends SIGINT; sleep terminates.
               Rush reaps it, takes the terminal back, and removes the job.
```

The job ID and PGID stay the same throughout this sequence. Moving between foreground and background changes terminal ownership and whether Rush waits; it does not create a replacement process.

## Boundaries of this learning version

The example handles one process per job. A pipeline needs per-process status tracking and an aggregate job state: one member stopping does not necessarily mean the whole pipeline has stopped.

Background statuses refresh before prompts and job builtins. They are not collected immediately while Rush is blocked waiting for another input line. A SIGCHLD-driven input loop can be added later.

New foreground launches also have a small timing window before terminal handoff; see [how-launch-works.md](how-launch-works.md). Finally, some programs do not retry interrupted reads after stop/resume. The earlier macOS test found this behavior with native `cat` under both Rush and system Bash. Use `sleep` first when learning the signal and ownership sequence.
