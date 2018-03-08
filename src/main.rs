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

mod cwd;
mod history;
mod parser;

use std::env;
use std::ffi::CString;
use std::path::PathBuf;
use std::process;
use std::result;

use nix::Error::Sys;
use nix::errno::Errno;
use nix::sys::wait::{self, WaitStatus};
use nix::unistd::{self, ForkResult};
use structopt::StructOpt;

use cwd::Cwd;
use history::History;

type Result<T> = result::Result<T, failure::Error>;

macro_rules! display {
    ($fmt:expr) => (eprintln!(concat!(env!("CARGO_PKG_NAME"), ": ", $fmt)));
    ($fmt:expr, $($arg:tt)*) => (eprintln!(concat!(env!("CARGO_PKG_NAME"), ": ", $fmt), $($arg)*));
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
        let mut argv = parser::parse_line(&line);
        if argv.is_empty() {
            continue;
        }

        let cmd = argv.remove(0);

        if cmd == "cd" {
            cwd = cwd.cd(argv)?;
            continue;
        }

        argv.insert(0, cmd);
        execute(argv)?;
    }

    Ok(())
}

fn execute(argv: Vec<String>) -> Result<()> {
    assert!(!argv.is_empty());
    debug!("forking to execute {:?}", argv);

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
            let cmd = argv[0].clone();
            let argv: Vec<CString> = argv.into_iter().map(|s| CString::new(s).unwrap()).collect();

            if cmd.contains('/') {
                let path = CString::new(cmd.as_bytes())?;
                execv(&path, &argv);
            } else {
                for mut path in PathIterator::new() {
                    path.push('/');
                    path.push_str(&cmd);
                    let path = CString::new(path)?;
                    execv(&path, &argv);
                }
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
        Err(e) => panic!("[child] {}", e),
    }
}

struct PathIterator {
    path: Vec<String>,
}

impl PathIterator {
    fn new() -> Self {
        Self {
            path: env::var("PATH").expect("PATH required").split(':').rfold(
                Vec::new(),
                |mut path, s| {
                    path.push(s.into());
                    path
                },
            ),
        }
    }
}

impl Iterator for PathIterator {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        self.path.pop()
    }
}
