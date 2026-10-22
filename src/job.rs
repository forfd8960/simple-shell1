use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    os::unix::process::CommandExt,
    process::{Child, Command},
};

use nix::{
    errno::Errno,
    sys::{
        signal::{
            SaFlags, SigAction, SigHandler, SigSet,
            Signal::{self, SIGCHLD, SIGCONT, SIGPIPE},
            killpg, sigaction,
        },
        termios::{Termios, tcgetattr},
        wait::{WaitPidFlag, WaitStatus, waitpid},
    },
    unistd::{Pid, getpgrp, getpid, setpgid, tcgetpgrp, tcsetpgrp},
};

use crate::{ShlResult, errors::ShellErrors};

const SHELL_SIGNALS: [Signal; 5] = [
    Signal::SIGINT,
    Signal::SIGQUIT,
    Signal::SIGTSTP,
    Signal::SIGTTIN,
    Signal::SIGTTOU,
];

#[derive(Debug, Clone, PartialEq)]
pub enum JobState {
    Running,
    Stopped,
    Done,
}

#[derive(Debug)]
pub struct Job {
    pub id: usize,
    pub cmd: String,
    pub pgid: Pid, // process group id
    pub state: JobState,
    pub child: Child,
    pub terminal_modes: Option<Termios>,
}

#[derive(Debug)]
pub struct JobManager {
    pub jobs: BTreeMap<usize, Job>,
    pub next_id: usize,
    pub tty: File,
    pub shell_pgid: Pid,
    pub shell_modes: Termios,
    pub old_signals: Vec<(Signal, SigAction)>,
}

impl JobManager {
    pub fn new() -> ShlResult<Self> {
        let tty = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
        let tty_pgid = tcgetpgrp(&tty).map_err(|e| ShellErrors::TTYError(format!("{e}")))?;

        if tty_pgid != getpgrp() {
            return Err(ShellErrors::InvalidArgs(format!(
                "start Rush in the foreground"
            )));
        }

        let modes = tcgetattr(&tty).map_err(|e| ShellErrors::TTYError(format!("{e}")))?;

        let ignore = SigAction::new(SigHandler::SigIgn, SaFlags::empty(), SigSet::empty());

        let mut manager = Self {
            jobs: BTreeMap::new(),
            next_id: 1,
            tty,
            shell_pgid: tty_pgid,
            shell_modes: modes,
            old_signals: Vec::new(),
        };

        for sig in SHELL_SIGNALS {
            let old = unsafe { sigaction(sig, &ignore) }
                .map_err(|e| ShellErrors::TTYError(format!("{e}")))?;
            manager.old_signals.push((sig, old));
        }

        if getpgrp() != getpid() {
            setpgid(Pid::from_raw(0), Pid::from_raw(0))
                .map_err(|e| ShellErrors::TTYError(format!("{e}")))?;
        }

        manager.shell_pgid = getpgrp();
        tcsetpgrp(&manager.tty, manager.shell_pgid)
            .map_err(|e| ShellErrors::TTYError(format!("{e}")))?;

        Ok(manager)
    }

    pub fn launch(&mut self, mut cmd: Command, job_text: &str, background: bool) -> ShlResult<()> {
        cmd.process_group(0);

        let defaults = SigAction::new(SigHandler::SigDfl, SaFlags::SA_RESTART, SigSet::empty());

        unsafe {
            cmd.pre_exec(move || {
                for sig in SHELL_SIGNALS.into_iter().chain([SIGPIPE, SIGCHLD]) {
                    sigaction(sig, &defaults)?;
                }
                Ok(())
            });
        }

        let child = cmd.spawn()?;
        let pgid = Pid::from_raw(child.id() as i32);

        let id = self.next_id;
        self.next_id += 1;
        self.jobs.insert(
            id,
            Job {
                id,
                cmd: job_text.to_owned(),
                pgid: pgid,
                state: JobState::Running,
                child,
                terminal_modes: None,
            },
        );

        if background {
            println!("[{id}, {pgid}]");
            return Ok(());
        }

        Ok(())
    }

    fn active_job(&self, id: usize) -> ShlResult<&Job> {
        self.jobs
            .get(&id)
            .filter(|job| job.state != JobState::Done)
            .ok_or_else(|| ShellErrors::JobError(format!("no active job")))
    }

    pub fn list(&mut self) -> ShlResult<()> {
        self.refresh()?;
        for job in self.jobs.values() {
            println!("[{}, {:?}, {}]", job.id, job.state, job.cmd);
        }

        Ok(())
    }

    pub fn refresh(&mut self) -> ShlResult<()> {
        let flags = WaitPidFlag::WNOHANG | WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED;

        for job in self.jobs.values_mut() {
            while job.state != JobState::Done {
                let pid = Pid::from_raw(job.child.id() as i32);

                match waitpid(pid, Some(flags)) {
                    Ok(WaitStatus::StillAlive) => break,
                    Ok(_status) => todo!(),
                    Err(Errno::EINTR) => continue,
                    Err(e) => return Err(ShellErrors::NixError(e)),
                }
            }
        }

        Ok(())
    }

    pub fn background(&mut self, id: usize) -> ShlResult<()> {
        self.refresh()?;
        let pgid = self.active_job(id)?.pgid;
        killpg(pgid, SIGCONT)?;

        match self.jobs.get_mut(&id) {
            Some(job) => {
                job.state = JobState::Running;
                println!("[{id}] Running\t{}", job.cmd);
            }
            None => {}
        }

        Ok(())
    }
}
