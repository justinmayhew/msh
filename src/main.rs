#![feature(iter_rfold)]

extern crate env_logger;
#[macro_use]
extern crate failure;
extern crate libc;
#[macro_use]
extern crate log;
extern crate nix;
#[macro_use]
extern crate structopt;

mod ast;
mod command;
mod cwd;
mod history;
mod lexer;
mod parser;

use std::ffi::CString;
use std::path::PathBuf;
use std::process;
use std::result;

use nix::Error::Sys;
use nix::errno::Errno;
use nix::sys::wait::{self, WaitStatus};
use nix::unistd::{self, ForkResult};
use structopt::StructOpt;

use ast::Statement;
use command::{Command, Execv};
use cwd::Cwd;
use history::History;

type Result<T> = result::Result<T, failure::Error>;

macro_rules! display {
    ($fmt:expr) => (eprintln!(concat!(env!("CARGO_PKG_NAME"), ": ", $fmt)));
    ($fmt:expr, $($arg:tt)*) => (eprintln!(concat!(env!("CARGO_PKG_NAME"), ": ", $fmt), $($arg)*));
}

fn display_err(e: &failure::Error) {
    let causes: Vec<String> = e.causes().map(|c| format!("{}", c)).collect();
    display!("{}", causes.join(": "));
}

#[derive(Debug, StructOpt)]
#[structopt(about = "A simple Unix shell")]
struct Options {
    #[structopt(long = "history", help = "Path to the history file", parse(from_os_str))]
    history: Option<PathBuf>,
}

fn main() {
    env_logger::init();

    let options = Options::from_args();

    let code = match repl(&options) {
        Ok(()) => 0,
        Err(e) => {
            display!("{}", e);
            1
        }
    };

    process::exit(code);
}

fn repl(options: &Options) -> Result<()> {
    let mut cwd = Cwd::new();
    let history = History::new(options.history.as_ref())?;

    while let Some(line) = history.readline(&format!("{} $ ", cwd.current().display()))? {
        let mut statements = parser::parse(&line)?;
        for statement in statements {
            match statement {
                Statement::Command(command) => {
                    if command.name() == "cd" {
                        if let Err(e) = cwd.cd(command.arguments()) {
                            display_err(&e);
                        }
                    } else {
                        execute(command)?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn execute(command: Command) -> Result<()> {
    debug!("forking to execute {:?}", command);

    match unistd::fork()? {
        ForkResult::Parent { child } => loop {
            match wait::waitpid(child, None) {
                Ok(WaitStatus::Exited(_, code)) => {
                    debug!("child exited with {} status code", code);
                    break;
                }
                Ok(status) => {
                    debug!("waitpid: {:?}", status);
                }
                Err(Sys(Errno::ECHILD)) => {
                    display!("child process was killed unexpectedly");
                    break;
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

    Ok(())
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
