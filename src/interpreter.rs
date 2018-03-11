use std::ffi::CString;
use std::process;

use nix::Error::Sys;
use nix::errno::Errno;
use nix::sys::wait::{self, WaitStatus};
use nix::unistd::{self, ForkResult};

use ast::{Block, Stmt};
use command::{Command, Execv};
use cwd::Cwd;
use {display_err, Result};

pub struct Interpreter {
    cwd: Cwd,
}

impl Interpreter {
    pub fn new() -> Self {
        Self { cwd: Cwd::new() }
    }

    pub fn execute(&mut self, block: Block) -> Result<()> {
        for stmt in block {
            match stmt {
                Stmt::If(stmt) => {
                    if execute(stmt.test)? == 0 {
                        self.execute(stmt.consequent)?;
                    } else if let Some(alternate) = stmt.alternate {
                        self.execute(alternate)?;
                    }
                }
                Stmt::Command(command) => {
                    if command.name() == "cd" {
                        if let Err(e) = self.cwd.cd(command.arguments()) {
                            display_err(&e);
                        }
                    } else {
                        execute(command)?;
                    }
                }
            }
        }

        Ok(())
    }

    pub fn cwd(&self) -> String {
        self.cwd.current().display().to_string()
    }
}

fn execute(command: Command) -> Result<i32> {
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

            match command.into_execv() {
                Execv::Exact(path, argv) => execv(&path, &argv),
                Execv::Relative(path_iterator, argv) => for path in path_iterator {
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
