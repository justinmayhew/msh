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

macro_rules! display {
    ($fmt:expr) => (eprintln!(concat!(env!("CARGO_PKG_NAME"), ": ", $fmt)));
    ($fmt:expr, $($arg:tt)*) => (eprintln!(concat!(env!("CARGO_PKG_NAME"), ": ", $fmt), $($arg)*));
}

mod ast;
mod command;
mod cwd;
mod history;
mod interpreter;
mod lexer;
mod parser;

use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::process;
use std::result;

use failure::ResultExt;
use structopt::StructOpt;

use history::History;
use interpreter::Interpreter;

type Result<T> = result::Result<T, failure::Error>;

fn display_err(e: &failure::Error) {
    let causes: Vec<String> = e.causes().map(|c| format!("{}", c)).collect();
    display!("{}", causes.join(": "));
}

#[derive(Debug, StructOpt)]
#[structopt(about = "A simple Unix shell")]
struct Options {
    #[structopt(help = "Executes the script file provided", parse(from_os_str))]
    file: Option<PathBuf>,
    #[structopt(long = "history", help = "Sets the history file location", parse(from_os_str))]
    history: Option<PathBuf>,
}

fn main() {
    env_logger::init();

    let code = match run(Options::from_args()) {
        Ok(()) => 0,
        Err(e) => {
            display_err(&e);
            1
        }
    };

    process::exit(code);
}

fn run(options: Options) -> Result<()> {
    match options.file {
        Some(path) => File::open(&path)
            .map_err(Into::into)
            .and_then(execute)
            .with_context(|_| path.display().to_string())
            .map_err(Into::into),
        None => if stdin_isatty() {
            repl(&options)
        } else {
            execute(io::stdin())
        },
    }
}

fn execute<R: io::Read>(mut reader: R) -> Result<()> {
    let mut src = String::new();
    reader.read_to_string(&mut src)?;

    let program = parser::parse(&src)?;
    Interpreter::new().execute(&program)
}

fn repl(options: &Options) -> Result<()> {
    let history = History::new(options.history.as_ref())?;
    let mut interpreter = Interpreter::new();

    while let Some(line) = history.readline(&format!("{} $ ", interpreter.cwd()))? {
        let statements = parser::parse(&line)?;
        interpreter.execute(&statements)?;
    }

    Ok(())
}

fn stdin_isatty() -> bool {
    unsafe { libc::isatty(libc::STDIN_FILENO) == 1 }
}
