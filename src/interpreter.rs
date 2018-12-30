use std::collections::HashSet;
use std::env;
use std::ffi::CString;
use std::fs::File;
use std::mem;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::io::{AsRawFd, RawFd};
use std::process;

use failure::ResultExt;
use libc;
use nix::errno::Errno;
use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, SigmaskHow, Signal};
use nix::sys::wait::{self, WaitPidFlag, WaitStatus};
use nix::unistd::{self, ForkResult, Pid};
use nix::Error::Sys;

use crate::ast::Stmt;
use crate::command::{Command, Execv, ExpandedCommand};
use crate::cwd::Cwd;
use crate::environment::Environment;
use crate::redirect::Redirect;
use crate::status::Status;
use crate::{print_error, Result};

extern "C" fn nothing(_: libc::c_int) {}

pub struct Interpreter {
    cwd: Cwd,
    env: Environment,
}

impl Interpreter {
    pub fn new() -> Result<Self> {
        // Set a signal handler for SIGCHLD so that it's not considered ignored.
        // sigwait(2) won't emit notifications for ignored signals on macOS.
        let action = SigAction::new(
            SigHandler::Handler(nothing),
            SaFlags::empty(),
            SigSet::empty(),
        );
        unsafe {
            signal::sigaction(Signal::SIGCHLD, &action)?;
        }

        Ok(Self {
            cwd: Cwd::new(),
            env: Environment::new(),
        })
    }

    pub fn execute(&mut self, block: &[Stmt]) -> Result<()> {
        for stmt in block {
            match *stmt {
                Stmt::If(ref stmt) => {
                    if self.execute_command(&stmt.test)?.is_success() {
                        self.execute(&stmt.consequent)?;
                    } else if let Some(ref alternate) = stmt.alternate {
                        self.execute(alternate)?;
                    }
                }
                Stmt::While(ref stmt) => {
                    while self.execute_command(&stmt.test)?.is_success() {
                        self.execute(&stmt.body)?;
                    }
                }
                Stmt::Export(ref exportables) => {
                    for exportable in exportables {
                        self.env.export(exportable)?;
                    }
                }
                Stmt::Assignment(ref pairs) => {
                    for pair in pairs {
                        self.env.assign(pair)?;
                    }
                }
                Stmt::Command(ref command) => {
                    self.execute_command(command)?;
                }
            }
        }

        Ok(())
    }

    pub fn cwd(&self) -> String {
        self.cwd.current().display().to_string()
    }

    fn execute_command(&mut self, command: &Command) -> Result<Status> {
        let command = command.expand(&self.env)?;

        match command.name().as_bytes() {
            b"cd" => {
                if command.pipeline().is_some() {
                    unimplemented!("builtin pipelines");
                }
                Ok(self.cwd.cd(self.env.home(), command.arguments()))
            }
            b"exit" => {
                if command.arguments().len() > 1 {
                    display!("exit: too many arguments");
                    return Ok(Status::Failure);
                }

                let code = match command.arguments().first() {
                    Some(arg) => arg
                        .to_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_else(|| {
                            display!("exit: numeric argument required");
                            2
                        }),
                    None => 0,
                };
                process::exit(code);
            }
            _ => Ok(execute(&command, &self.env)),
        }
    }
}

fn execute(cmd: &ExpandedCommand, env: &Environment) -> Status {
    let (mut pids, last_pid) = spawn_children(cmd, env);

    let mut sigset = SigSet::empty();
    sigset.add(Signal::SIGINT);
    sigset.add(Signal::SIGQUIT);
    sigset.add(Signal::SIGCHLD);

    signal::sigprocmask(SigmaskHow::SIG_BLOCK, Some(&sigset), None)
        .expect("failed blocking signals");

    let mut status = Status::Success;
    'outer: loop {
        let signal = sigset.wait().expect("failed waiting for signal");
        match signal {
            Signal::SIGINT | Signal::SIGQUIT => debug!("ignoring {:?}", signal),
            Signal::SIGCHLD => loop {
                match wait::waitpid(None, Some(WaitPidFlag::WNOHANG)) {
                    Ok(WaitStatus::Exited(pid, code)) => {
                        debug!("PID {} returned {}", pid, code);
                        assert!(pids.remove(&pid));
                        if pid == last_pid {
                            status = code.into();
                        }
                    }
                    Ok(WaitStatus::Signaled(pid, signal, _)) => {
                        debug!("PID {} received {:?}", pid, signal);
                        assert!(pids.remove(&pid));
                        if pid == last_pid {
                            status = Status::Failure;
                        }
                    }
                    Ok(WaitStatus::StillAlive) => break,
                    Ok(status) => debug!("wait: {:?}", status),
                    Err(Sys(Errno::ECHILD)) => break 'outer,
                    Err(e) => panic!("wait: {}", e),
                }
            },
            signal => panic!("received unexpected {:?}", signal),
        }
    }

    assert!(pids.is_empty());

    signal::sigprocmask(SigmaskHow::SIG_UNBLOCK, Some(&sigset), None)
        .expect("failed unblocking signals");

    status
}

fn spawn_children(cmd: &ExpandedCommand, env: &Environment) -> (HashSet<Pid>, Pid) {
    let mut pids = HashSet::new();
    let mut next_cmd = Some(cmd);
    let mut next_stdin = None;

    while let Some(cmd) = next_cmd {
        let (stdin, stdout) = match cmd.pipeline() {
            Some(next) => {
                next_cmd = Some(next);
                let (read, write) = unistd::pipe().expect("failed creating pipe");
                (mem::replace(&mut next_stdin, Some(read)), Some(write))
            }
            None => {
                next_cmd = None;
                (next_stdin, None)
            }
        };

        match unistd::fork().expect("failed to fork") {
            ForkResult::Parent { child } => {
                pids.insert(child);
                if let Some(fd) = stdin {
                    unistd::close(fd).expect("failed closing read end of pipe");
                }
                if let Some(fd) = stdout {
                    unistd::close(fd).expect("failed closing write end of pipe");
                }
                if next_cmd.is_none() {
                    return (pids, child);
                }
            }
            ForkResult::Child => {
                if let Err(e) = execute_child(cmd, env, stdin, stdout) {
                    print_error(&e);
                }
                process::exit(1);
            }
        }
    }
    unreachable!();
}

fn execute_child(
    cmd: &ExpandedCommand,
    environment: &Environment,
    stdin: Option<RawFd>,
    stdout: Option<RawFd>,
) -> Result<()> {
    if let Some(fd) = stdin {
        unistd::dup2(fd, libc::STDIN_FILENO)?;
    }
    if let Some(fd) = stdout {
        unistd::dup2(fd, libc::STDOUT_FILENO)?;
    }

    for redirect in cmd.redirects() {
        match *redirect {
            Redirect::InFile(ref path) => {
                let file =
                    File::open(path).with_context(|_| path.to_string_lossy().into_owned())?;
                unistd::dup2(file.as_raw_fd(), libc::STDIN_FILENO)?;
            }
            Redirect::OutErr => {
                unistd::dup2(libc::STDERR_FILENO, libc::STDOUT_FILENO)?;
            }
            Redirect::OutFile(ref path, mode) => {
                let file = mode.open(path)?;
                unistd::dup2(file.as_raw_fd(), libc::STDOUT_FILENO)?;
            }
            Redirect::ErrOut => {
                unistd::dup2(libc::STDOUT_FILENO, libc::STDERR_FILENO)?;
            }
            Redirect::ErrFile(ref path, mode) => {
                let file = mode.open(path)?;
                unistd::dup2(file.as_raw_fd(), libc::STDERR_FILENO)?;
            }
        }
    }

    match cmd.clone().into_execv(environment) {
        Execv::Exact(path, argv, env) => execve(&path, &argv, &env),
        Execv::Relative(name, argv, env) => {
            for mut path in env::split_paths(environment.path()) {
                path.push(&name);
                let path = CString::new(path.into_os_string().into_vec()).unwrap();
                execve(&path, &argv, &env);
            }
        }
    }

    display!("command not found: {}", cmd.name().to_string_lossy());
    process::exit(1)
}

fn execve(path: &CString, argv: &[CString], env: &[CString]) {
    match unistd::execve(path, argv, env) {
        Ok(_) => unreachable!(),
        Err(Sys(Errno::ENOENT)) => {}
        Err(e) => {
            display!("{}: {}", path.to_string_lossy(), e);
            process::exit(1);
        }
    }
}
