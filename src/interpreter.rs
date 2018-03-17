use std::env;
use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::OsStringExt;
use std::process;

use nix::Error::Sys;
use nix::errno::Errno;
use nix::sys::wait::{self, WaitStatus};
use nix::unistd::{self, ForkResult};

use ast::Stmt;
use command::{Command, Execv};
use cwd::Cwd;
use {display_err, Result};

pub struct Interpreter {
    cwd: Cwd,
    path: OsString,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            cwd: Cwd::new(),
            path: env::var_os("PATH").unwrap_or_default(),
        }
    }

    pub fn execute(&mut self, block: &[Stmt]) -> Result<()> {
        for stmt in block {
            match *stmt {
                Stmt::If(ref stmt) => {
                    if self.execute_command(&stmt.test)? == 0 {
                        self.execute(&stmt.consequent)?;
                    } else if let Some(ref alternate) = stmt.alternate {
                        self.execute(alternate)?;
                    }
                }
                Stmt::While(ref stmt) => while self.execute_command(&stmt.test)? == 0 {
                    self.execute(&stmt.body)?;
                },
                Stmt::Command(ref command) => {
                    if command.name() == "cd" {
                        if let Err(e) = self.cwd.cd(command.arguments()) {
                            display_err(&e);
                        }
                    } else {
                        self.execute_command(command)?;
                    }
                }
            }
        }

        Ok(())
    }

    pub fn cwd(&self) -> String {
        self.cwd.current().display().to_string()
    }

    fn execute_command(&self, command: &Command) -> Result<i32> {
        execute(command, &self.path)
    }
}

fn execute(command: &Command, path: &OsStr) -> Result<i32> {
    debug!("forking to execute {:?}", command);

    match unistd::fork()? {
        ForkResult::Parent { child } => loop {
            match wait::waitpid(child, None) {
                Ok(WaitStatus::Exited(_, code)) => {
                    debug!("child exited with {} status code", code);
                    return Ok(code);
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
            let cmd = command.name().to_string();

            match command.clone().into_execv() {
                Execv::Exact(path, argv) => execv(&path, &argv),
                Execv::Relative(name, argv) => for mut path in env::split_paths(path) {
                    path.push(&name);
                    let path = CString::new(path.into_os_string().into_vec()).unwrap();
                    execv(&path, &argv);
                },
            }

            display!("command not found: {}", cmd);
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
