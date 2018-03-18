use std::env;
use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::process;

use nix::Error::Sys;
use nix::errno::Errno;
use nix::sys::wait::{self, WaitStatus};
use nix::unistd::{self, ForkResult};

use {display_err, Result};
use ast::Stmt;
use command::{Command, Execv, ExpandedCommand};
use cwd::Cwd;
use status::Status;

pub struct Interpreter {
    cwd: Cwd,
    path: OsString,
    home: PathBuf,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            cwd: Cwd::new(),
            path: env::var_os("PATH").unwrap_or_default(),
            home: env::home_dir().expect("HOME required"),
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
        let command = match command.expand(&self.home) {
            Ok(command) => command,
            Err(e) => {
                display_err(&e);
                return Ok(Status::Failure);
            }
        };

        if command.name().as_bytes() == b"cd" {
            Ok(self.cwd.cd(&self.home, command.arguments()))
        } else {
            execute(&command, &self.path)
        }
    }
}

fn execute(command: &ExpandedCommand, path: &OsStr) -> Result<Status> {
    debug!("forking to execute {:?}", command);

    match unistd::fork()? {
        ForkResult::Parent { child } => loop {
            match wait::waitpid(child, None) {
                Ok(WaitStatus::Exited(_, code)) => {
                    debug!("child exited with {} status code", code);
                    return Ok(code.into());
                }
                Ok(status) => {
                    debug!("waitpid: {:?}", status);
                }
                Err(Sys(Errno::ECHILD)) => {
                    unimplemented!("child process was killed unexpectedly");
                }
                Err(e) => {
                    panic!("waitpid: {}", e);
                }
            }
        },
        ForkResult::Child => {
            match command.clone().into_execv() {
                Execv::Exact(path, argv) => execv(&path, &argv),
                Execv::Relative(name, argv) => for mut path in env::split_paths(path) {
                    path.push(&name);
                    let path = CString::new(path.into_os_string().into_vec()).unwrap();
                    execv(&path, &argv);
                },
            }

            display!("command not found: {}", command.name().to_string_lossy());
            process::exit(1);
        }
    }
}

fn execv(path: &CString, argv: &[CString]) {
    debug!("[child] execv {:?} {:?}", path, argv);
    match unistd::execv(path, argv) {
        Ok(_) => unreachable!(),
        Err(Sys(Errno::ENOENT)) => {}
        Err(e) => {
            display!("{}: {}", path.to_string_lossy(), e);
            process::exit(1);
        }
    }
}
