#![feature(iter_rfold)]

extern crate env_logger;
extern crate failure;
#[macro_use]
extern crate log;
extern crate nix;

use std::env;
use std::ffi::CString;
use std::io::{self, Write};
use std::process;
use std::result;

use nix::Error::Sys;
use nix::errno::Errno;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{execv, fork, ForkResult};

type Result<T> = result::Result<T, failure::Error>;

macro_rules! display {
    ($fmt:expr) => (eprintln!(concat!(env!("CARGO_PKG_NAME"), ": ", $fmt)));
    ($fmt:expr, $($arg:tt)*) => (eprintln!(concat!(env!("CARGO_PKG_NAME"), ": ", $fmt), $($arg)*));
}

fn main() {
    env_logger::init();

    let code = match repl() {
        Ok(()) => 0,
        Err(e) => {
            display!("{}", e);
            1
        }
    };

    process::exit(code);
}

fn repl() -> Result<()> {
    loop {
        prompt()?;
        let mut line = String::new();
        let n = io::stdin().read_line(&mut line)?;
        if n == 0 {
            break;
        }

        let argv = parse_line(&line);
        if argv.is_empty() {
            continue;
        }

        execute(argv)?;
    }

    Ok(())
}

fn prompt() -> Result<()> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    lock.write_all(b"$ ")?;
    lock.flush()?;
    Ok(())
}

fn parse_line(line: &str) -> Vec<String> {
    line.split_whitespace().map(String::from).collect()
}

fn execute(argv: Vec<String>) -> Result<()> {
    assert!(!argv.is_empty());
    debug!("forking to execute {:?}", argv);

    match fork()? {
        ForkResult::Parent { child } => loop {
            match waitpid(child, None) {
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

            for mut path in PathIterator::new() {
                path.push('/');
                path.push_str(&cmd);
                let path = CString::new(path).unwrap();

                debug!("[child] execv {:?} {:?}", path, argv);
                match execv(&path, &argv) {
                    Ok(_) => unreachable!(),
                    Err(Sys(Errno::ENOENT)) => {}
                    Err(e) => panic!("[child] {}", e),
                }
            }

            display!("command not found: {}", cmd);
            process::exit(1);
        }
    }

    Ok(())
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
