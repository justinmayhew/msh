use std::env;
use std::ffi::CString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::process;

use nix::Error::Sys;
use nix::errno::Errno;
use nix::sys::wait::{self, WaitStatus};
use nix::unistd::{self, ForkResult};

use ast::Stmt;
use command::{Command, Execv, ExpandedCommand};
use cwd::Cwd;
use environment::Environment;
use status::Status;
use {display_err, Result};

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
        let command = match command.expand(&self.env) {
            Ok(command) => command,
            Err(e) => {
                display_err(&e);
                return Ok(Status::Failure);
            }
        };

        if command.name().as_bytes() == b"cd" {
            Ok(self.cwd.cd(self.env.home(), command.arguments()))
        } else {
            execute(&command, &self.env)
        }
    }
}

fn execute(command: &ExpandedCommand, environment: &Environment) -> Result<Status> {
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
            match command.clone().into_execv(environment) {
                Execv::Exact(path, argv, env) => execve(&path, &argv, &env),
                Execv::Relative(name, argv, env) => {
                    for mut path in env::split_paths(environment.path()) {
                        path.push(&name);
                        let path = CString::new(path.into_os_string().into_vec()).unwrap();
                        execve(&path, &argv, &env);
                    }
                }
            }

            display!("command not found: {}", command.name().to_string_lossy());
            process::exit(1);
        }
    }
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
