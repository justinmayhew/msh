use std::env;
use std::ffi::CString;
use std::mem;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::io::RawFd;
use std::process;

use failure::ResultExt;
use libc;
use nix::Error::Sys;
use nix::errno::Errno;
use nix::sys::wait::{self, WaitStatus};
use nix::unistd::{self, ForkResult, Pid};

use Result;
use ast::Stmt;
use command::{Command, Execv, ExpandedCommand};
use cwd::Cwd;
use environment::Environment;
use status::Status;

pub struct Interpreter {
    cwd: Cwd,
    env: Environment,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            cwd: Cwd::new(),
            env: Environment::new(),
        }
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
                Stmt::While(ref stmt) => while self.execute_command(&stmt.test)?.is_success() {
                    self.execute(&stmt.body)?;
                },
                Stmt::Export(ref exportables) => for exportable in exportables {
                    self.env.export(exportable)?;
                },
                Stmt::Assignment(ref pairs) => for pair in pairs {
                    self.env.assign(pair)?;
                },
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

        if command.name().as_bytes() == b"cd" {
            if command.pipeline().is_some() {
                unimplemented!("builtin pipelines");
            }
            Ok(self.cwd.cd(self.env.home(), command.arguments()))
        } else {
            execute(&command, &self.env)
        }
    }
}

fn execute(cmd: &ExpandedCommand, env: &Environment) -> Result<Status> {
    let (mut running_children, last_pid) = spawn_children(cmd, env)?;
    let mut status = Status::Success;

    while running_children > 0 {
        match wait::wait() {
            Ok(WaitStatus::Exited(pid, code)) => {
                debug!("child {} exited with {} status code", pid, code);
                running_children -= 1;
                if pid == last_pid {
                    status = code.into();
                }
            }
            Ok(status) => {
                debug!("wait: {:?}", status);
            }
            Err(Sys(Errno::ECHILD)) => {
                unimplemented!("child process was killed unexpectedly");
            }
            Err(e) => {
                panic!("wait: {}", e);
            }
        }
    }

    Ok(status)
}

fn spawn_children(cmd: &ExpandedCommand, env: &Environment) -> Result<(u16, Pid)> {
    let mut count = 0;
    let mut next_cmd = Some(cmd);
    let mut next_stdin = None;

    while let Some(cmd) = next_cmd {
        count += 1;

        let (stdin, stdout) = match cmd.pipeline() {
            Some(next) => {
                next_cmd = Some(next);
                let (read, write) = unistd::pipe().context("failed creating pipe")?;
                (mem::replace(&mut next_stdin, Some(read)), Some(write))
            }
            None => {
                next_cmd = None;
                (next_stdin, None)
            }
        };

        debug!("forking to execute {:?}", cmd);
        match unistd::fork().context("failed to fork")? {
            ForkResult::Parent { child } => {
                if let Some(fd) = stdin {
                    unistd::close(fd).context("failed closing read end of pipe")?;
                }
                if let Some(fd) = stdout {
                    unistd::close(fd).context("failed closing write end of pipe")?;
                }
                if next_cmd.is_none() {
                    return Ok((count, child));
                }
            }
            ForkResult::Child => execute_child(cmd, env, stdin, stdout),
        }
    }
    unreachable!();
}

fn execute_child(
    cmd: &ExpandedCommand,
    environment: &Environment,
    stdin: Option<RawFd>,
    stdout: Option<RawFd>,
) -> ! {
    if let Some(fd) = stdin {
        unistd::dup2(fd, libc::STDIN_FILENO).expect("replacing stdin failed");
    }
    if let Some(fd) = stdout {
        unistd::dup2(fd, libc::STDOUT_FILENO).expect("replacing stdout failed");
    }

    match cmd.clone().into_execv(environment) {
        Execv::Exact(path, argv, env) => execve(&path, &argv, &env),
        Execv::Relative(name, argv, env) => for mut path in env::split_paths(environment.path()) {
            path.push(&name);
            let path = CString::new(path.into_os_string().into_vec()).unwrap();
            execve(&path, &argv, &env);
        },
    }

    display!("command not found: {}", cmd.name().to_string_lossy());
    process::exit(1)
}

fn execve(path: &CString, argv: &[CString], env: &[CString]) {
    debug!("[child] execve {:?} {:?}", path, argv);
    match unistd::execve(path, argv, env) {
        Ok(_) => unreachable!(),
        Err(Sys(Errno::ENOENT)) => {}
        Err(e) => {
            display!("{}: {}", path.to_string_lossy(), e);
            process::exit(1);
        }
    }
}
