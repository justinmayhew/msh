#![feature(iter_rfold)]

extern crate env_logger;
extern crate failure;
#[macro_use]
extern crate log;
extern crate nix;

mod parser;

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

struct DirStatus {
    path: String,
    last: Option<String>,
}

impl DirStatus {
    fn new() -> Self {
        let pwd = env::current_dir().unwrap();
        Self {
            path: pwd.to_str().unwrap().into(),
            last: None,
        }
    }

    fn current(&self) -> &str {
        &self.path
    }

    fn cd(self, mut argv: Vec<String>) -> Result<Self> {
        assert!(argv.len() <= 1);

        let mut path = argv.pop()
            .unwrap_or_else(|| env::var("HOME").expect("HOME required"));

        if path == "-" {
            path = match self.last {
                Some(path) => path,
                None => self.path.clone(),
            };
        }

        env::set_current_dir(&path)?;

        Ok(DirStatus {
            path,
            last: Some(self.path),
        })
    }
}

fn repl() -> Result<()> {
    let mut dir = DirStatus::new();

    loop {
        prompt(dir.current())?;

        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            break;
        }

        let mut argv = parser::parse_line(&line);
        if argv.is_empty() {
            continue;
        }

        let cmd = argv.remove(0);

        if cmd == "cd" {
            dir = dir.cd(argv)?;
            continue;
        }

        argv.insert(0, cmd);
        execute(argv)?;
    }

    Ok(())
}

fn prompt(cwd: &str) -> Result<()> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    write!(lock, "{} $ ", cwd)?;
    lock.flush()?;
    Ok(())
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
