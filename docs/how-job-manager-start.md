# How `JobManager::new()` works

`JobManager::new()` prepares Rush to manage jobs and control its terminal. It does not launch a command. It checks the terminal, creates an empty job table, configures the shell's signal behavior, and makes sure Rush has its own foreground process group.

This explanation follows the constructor in [job-implement.md](job-implement.md), which uses Rust's standard library and `nix`.

## 1. Return either a manager or an error

```rust
pub fn new() -> io::Result<Self> {
    // Initialization steps...
}
```

`Self` means `JobManager`. `io::Result<Self>` means the function returns either `Ok(manager)` or an I/O error.

Terminal and process-group setup can fail, so the caller uses `?` to propagate initialization errors:

```rust
let jobs = JobManager::new()?;
```

## 2. Check that stdin is a terminal

```rust
if !io::stdin().is_terminal() {
    return Err(invalid(
        "this learning version requires an interactive terminal",
    ));
}
```

`is_terminal()` checks whether standard input is connected to a terminal. This version expects someone to type commands interactively, including Ctrl+C and Ctrl+Z.

For example, running Rush normally in a terminal is supported. Piping a script into it is deliberately rejected by this learning version. A more complete shell could have a separate noninteractive mode that skips terminal job-control setup.

`invalid()` is the guide's helper for creating an `io::Error` with an explanatory message.

## 3. Open the controlling terminal

```rust
let tty = OpenOptions::new()
    .read(true)
    .write(true)
    .open("/dev/tty")?;
```

`/dev/tty` refers to the current process's controlling terminal. Opening it produces a `File` handle that the manager can keep for terminal-control operations.

Later, Rush uses this handle to:

- Find or change the terminal's foreground process group.
- Read terminal settings such as echo and input modes.
- Restore those settings when a foreground job stops or exits.

This is a dedicated terminal handle, so command redirections do not replace it. The REPL still reads command lines through stdin; the `tty` field is used for terminal control.

The `?` returns an error if opening `/dev/tty` fails. Having terminal stdin does not guarantee that a controlling terminal is available, so this operation still needs error handling.

## 4. Check that Rush starts in the foreground

```rust
if tcgetpgrp(&tty).map_err(io_error)? != getpgrp() {
    return Err(invalid("start Rush in the foreground"));
}
```

These two functions answer different questions:

| Function | What it returns |
| --- | --- |
| `tcgetpgrp(&tty)` | The process group currently in the terminal's foreground |
| `getpgrp()` | The process group that Rush currently belongs to |

If the values match, Rush belongs to the foreground group and may proceed with initialization. It does not yet need to be the group leader.

If they differ, another group owns the terminal. This simple constructor returns an error instead of taking the terminal away from that group. For this lesson, start Rush with `cargo run`, rather than putting it in the background with `cargo run &`.

`tcgetpgrp()` returns a `nix` error on failure. `map_err(io_error)` converts that error to `std::io::Error`, and `?` propagates it.

## 5. Save the terminal's initial settings

```rust
let shell_modes = tcgetattr(&tty).map_err(io_error)?;
```

`tcgetattr()` reads the terminal configuration into a `Termios` value. The settings include whether typed characters are echoed and whether input is collected into lines.

Foreground programs may change these settings. Rush saves its initial settings so it can restore the prompt's expected behavior after a job stops or exits.

This saves terminal settings, not screen contents or typed command history.

## 6. Create the manager's initial state

```rust
let mut manager = Self {
    jobs: BTreeMap::new(),
    next_id: 1,
    tty,
    shell_pgid: getpgrp(),
    shell_modes,
    old_signals: Vec::new(),
};
```

Each field has a specific purpose:

| Field | Initial value and purpose |
| --- | --- |
| `jobs` | An empty job table; no commands have been registered yet. |
| `next_id` | `1`, so the first registered job gets ID `%1`. |
| `tty` | The owned handle to the controlling terminal. |
| `shell_pgid` | Rush's current group ID; updated after group setup below. |
| `shell_modes` | The terminal settings saved in step 5. |
| `old_signals` | An empty list that will hold the previous signal dispositions. |

`BTreeMap` keeps job IDs ordered when listing jobs. `mut` allows the remaining setup steps to update the manager.

Creating the manager before changing signals also gives initialization a cleanup path: if a later `?` returns an error, the local manager is dropped. Its `Drop` implementation attempts to restore the saved terminal state and any signal dispositions already recorded.

## 7. Prepare the ignore-signal configuration

```rust
let ignore = SigAction::new(
    SigHandler::SigIgn,
    SaFlags::empty(),
    SigSet::empty(),
);
```

This constructs a signal-action configuration. It does not install the configuration yet.

- `SigIgn` means ignore the signal.
- `SaFlags::empty()` selects no extra signal-action flags.
- `SigSet::empty()` adds no extra signals to the action's signal mask.

Rush will use this same configuration for the signals in `SHELL_SIGNALS`.

## 8. Ignore the shell's job-control signals and save their old settings

```rust
for signal in SHELL_SIGNALS {
    let old = unsafe { sigaction(signal, &ignore) }.map_err(io_error)?;
    manager.old_signals.push((signal, old));
}
```

The guide defines `SHELL_SIGNALS` as:

```rust
const SHELL_SIGNALS: [Signal; 5] = [
    Signal::SIGINT,
    Signal::SIGQUIT,
    Signal::SIGTSTP,
    Signal::SIGTTIN,
    Signal::SIGTTOU,
];
```

| Signal | Why Rush ignores it in this learning version |
| --- | --- |
| `SIGINT` | Ctrl+C at the prompt should not terminate Rush. |
| `SIGQUIT` | A terminal-generated quit signal should not terminate Rush. |
| `SIGTSTP` | Ctrl+Z at the prompt should not suspend Rush. |
| `SIGTTIN` | Protect Rush from being stopped by background terminal reads during job-control operations. |
| `SIGTTOU` | Allow Rush to change terminal ownership/settings while its group is temporarily in the background. |

Ignoring SIGTTIN does not make background terminal reads succeed; they can fail instead. Rush should regain foreground ownership before reading another command.

Each `sigaction()` call installs the ignore disposition and returns the previous configuration. The manager saves `(signal, old)` so its `Drop` implementation can restore that signal later.

`unsafe` marks the care required when changing signal dispositions. This code installs a fixed operating-system disposition, not a Rust callback, and the learning shell remains single-threaded.

These changes affect Rush's own process. They do not change its parent shell's handlers. Before executing a command, `launch()` resets the child's signal dispositions so Ctrl+C and Ctrl+Z work normally for that command. See [how-launch-works.md](how-launch-works.md).

## 9. Leave SIGPIPE and SIGCHLD alone in the parent

```rust
// Rust may ignore SIGPIPE; do not change it in the parent here.
// Leave SIGCHLD waitable: do not install SIG_IGN or SA_NOCLDWAIT.
```

These comments explain two intentional choices:

- The constructor does not change Rush's SIGPIPE disposition. `launch()` resets it to the default in child commands, where normal pipe behavior is needed.
- The constructor does not explicitly ignore SIGCHLD or enable automatic child reaping with `SA_NOCLDWAIT`. Rush needs to collect child statuses itself through `waitpid()`.

This simple version polls for child status changes rather than installing a SIGCHLD notification handler. It assumes normal waitable child behavior on entry.

## 10. Give Rush its own process group if necessary

```rust
if getpgrp() != getpid() {
    setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(io_error)?;
}
```

`getpid()` returns Rush's process ID. `getpgrp()` returns its current process-group ID.

If those values already match, Rush is the leader of its own process group and no change is needed. Otherwise, `setpgid(0, 0)` puts Rush into a group whose ID equals Rush's PID:

```text
First 0: operate on the calling process, Rush.
Second 0: use that process's PID as the process-group ID.
```

`Pid::from_raw(0)` wraps each special zero value in the type expected by `nix`.

For example, Rush might initially have PID `5000` while belonging to PGID `4000`. After successful setup, Rush has PID `5000` and PGID `5000`.

The shell and its external jobs can then use separate groups. Giving a job the terminal directs terminal-generated signals to that job's group without including Rush.

Signal ignoring happens before this step because changing groups can temporarily leave Rush outside the terminal's existing foreground group.

## 11. Store the resulting group ID and claim the terminal

```rust
manager.shell_pgid = getpgrp();
tcsetpgrp(&manager.tty, manager.shell_pgid).map_err(io_error)?;
```

The first line records Rush's group after setup. This is the group ID Rush will use when reclaiming the terminal from jobs.

The second line makes that group the terminal's foreground process group. Creating a process group and giving it terminal ownership are separate operations; `setpgid()` alone does not perform both.

Rush is now ready to read commands from its prompt. Later, foreground execution temporarily gives the terminal to a job's PGID, then restores it to `shell_pgid`. See [how-fore-background-works.md](how-fore-background-works.md).

## 12. Return the initialized manager

```rust
Ok(manager)
```

The manager moves to the caller. It owns the terminal handle, empty job table, saved terminal settings, and saved signal dispositions.

After successful initialization:

- Rush has its own foreground process group.
- The shell has the signal behavior needed for this job-control lesson.
- The first job will receive ID `1`.
- Terminal and signal settings are available for later restoration.

No external command has been started yet. That happens when the executor calls `launch()`.
