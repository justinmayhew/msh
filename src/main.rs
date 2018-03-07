#![feature(iter_rfold)]

extern crate env_logger;
extern crate failure;
extern crate libc;
#[macro_use]
extern crate log;
extern crate nix;

mod parser;
mod history;

use std::borrow::Cow;
use std::env;
use std::ffi::CString;
use std::path::PathBuf;
use std::process;
use std::result;

use nix::Error::Sys;
use nix::errno::Errno;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{execv, fork, ForkResult};

use history::History;

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
    path: PathBuf,
    last: Option<PathBuf>,
}

impl DirStatus {
    fn new() -> Self {
        Self {
            path: env::current_dir().unwrap(),
            last: None,
        }
    }

    fn current(&self) -> Cow<str> {
        self.path.to_string_lossy()
    }

    fn cd(self, mut argv: Vec<String>) -> Result<Self> {
        assert!(argv.len() <= 1);

        let path = match argv.pop() {
            Some(path) => {
                if path == "-" {
                    match self.last {
                        Some(last) => last,
                        None => self.path.clone(),
                    }
                } else {
                    str_to_pathbuf(&path)
                }
            }
            None => env::home_dir().expect("HOME required"),
        };

        env::set_current_dir(&path)?;

        let absolute = if path.is_relative() {
            let mut buf = self.path.clone();
            buf.push(path);
            buf.canonicalize().expect("error canonicalizing path")
        } else {
            path
        };

        Ok(DirStatus {
            path: absolute,
            last: Some(self.path),
        })
    }
}

fn str_to_pathbuf(s: &str) -> PathBuf {
    let mut buf = PathBuf::new();
    buf.push(s);
    buf
}

fn repl() -> Result<()> {
    let mut dir = DirStatus::new();
    let history = History::new("/tmp/msh_history");

    while let Some(line) = history.readline(&format!("{} $ ", dir.current())) {
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
