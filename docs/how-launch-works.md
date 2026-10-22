# How `JobManager::launch()` works

`launch()` starts a command, registers it as a job, and decides whether to wait for it. This explanation follows the one-process-per-job implementation in [job-implement.md](job-implement.md).

## 1. Receive the command and execution options

```rust
pub fn launch(
    &mut self,
    mut command: Command,
    text: &str,
    background: bool,
) -> io::Result<()>
```

- `command`: the prepared program, arguments, and redirections.
- `text`: text to display in `jobs`, such as `"sleep 30 &"`.
- `background`: `true` when the user added `&`.
- `&mut self`: allows updating the job table.

A `Command` is configuration at this point—the process has not started. It is mutable because the function still needs to configure its process group and signal setup.

## 2. Configure a new process group

```rust
command.process_group(0);
```

When spawned, the child will start its own process group. `0` means its PGID will equal its PID.

This lets Rush signal the job's group without signalling itself. It also lets Rush give that group terminal ownership for foreground execution.

## 3. Prepare normal signal behavior for the child

```rust
let defaults = SigAction::new(
    SigHandler::SigDfl,
    SaFlags::SA_RESTART,
    SigSet::empty(),
);
```

- `SigDfl`: use the operating system's default action.
- `SA_RESTART`: request automatic restarting of certain interrupted system calls where applicable.
- `SigSet::empty()`: no additional signals are blocked while a handler runs.

Rush ignores some signals to protect itself. Its child needs normal behavior: Ctrl+C should terminate `sleep`, and Ctrl+Z should stop it.

## 4. Register a hook to reset signals in the child

```rust
unsafe {
    command.pre_exec(move || {
        for signal in SHELL_SIGNALS.into_iter().chain([
            Signal::SIGPIPE,
            Signal::SIGCHLD,
        ]) {
            sigaction(signal, &defaults).map_err(io_error)?;
        }
        Ok(())
    });
}
```

This registers the closure; it does not run it immediately.

During `spawn()`, the closure runs in the child, just before the requested program replaces the child's process image. `move` gives the closure ownership of `defaults`.

`SHELL_SIGNALS` contains the signals that Rush ignores for its own protection. Resetting them in the child ensures the command gets their normal behavior. Ignored signal dispositions can survive exec, so merely starting a different program is not enough.

`sigaction()` returns a `nix` error if setup fails. `map_err(io_error)` converts it to `std::io::Error`, and `?` returns that error from the hook. A hook error makes `spawn()` fail.

`unsafe` is needed because this stage has strict restrictions: avoid allocation, printing, locking, or normal shell logic. This hook only resets fixed signal dispositions. Keep this learning shell single-threaded.

## 5. Start the process and obtain its PGID

```rust
let child = command.spawn()?;
let pgid = Pid::from_raw(child.id() as i32);
```

`spawn()` starts the command and returns a `Child` handle without waiting for completion. If spawning fails, `?` returns the error to the caller.

Because step 2 made the child its group leader:

```text
child PID = job PGID
```

`child.id()` returns the process ID as a `u32`. The cast converts it to the integer representation used here, and `Pid::from_raw()` wraps it in the type expected by `nix`.

## 6. Assign a shell job ID

```rust
let id = self.next_id;
self.next_id += 1;
```

This is Rush's own counter. It is separate from the operating system's IDs:

```text
Job ID: 1       Used by: fg %1
PGID:   12345   Used by: killpg(...)
```

Increasing `next_id` prepares a different ID for the next job.

## 7. Register the job

```rust
self.jobs.insert(id, Job {
    id,
    command: text.to_owned(),
    pgid,
    state: JobState::Running,
    child,
    terminal_modes: None,
});
```

The manager now owns the `Child` handle and can track the process's status.

`text.to_owned()` creates a `String` that remains valid after `launch()` returns. `terminal_modes` starts as `None` because no settings have been captured from a stopped job yet.

`Running` is the initial recorded state. The process might already have exited or stopped; later waiting or refreshing discovers its actual status.

Once registered, this child must be waited on through the manager's `waitpid()` calls. Do not also call `Child::wait()` or `try_wait()` for it, because competing waiters can consume each other's status information.

## 8. Return immediately or wait in the foreground

```rust
if background {
    println!("[{id}] {pgid}");
    Ok(())
} else {
    self.wait_foreground(id)
}
```

For `sleep 30 &`, Rush prints the IDs and returns to its prompt. The child continues running while Rush keeps terminal ownership.

For `sleep 30`, `wait_foreground()` gives the job terminal ownership, waits until it stops or exits, and then restores the terminal to Rush. A stopped job stays in the job table so `fg` or `bg` can resume it. A completed foreground job is removed.

`launch()` handles starting and registering; `wait_foreground()` handles terminal handoff and waiting.

## A limit of this simple version

The child can start running before Rush calls `tcsetpgrp()` inside `wait_foreground()`. An interactive program might attempt an early terminal read and stop with SIGTTIN. If that happens, `fg %N` can resume it after handoff.

Do not add a blocking startup barrier or `SIGSTOP` inside `pre_exec()` to try to solve this: `spawn()` can wait for the child's exec handshake, leaving the parent unable to return and release the child. The guide deliberately postpones robust launch synchronization until after the basic one-process version is understood.
