#![feature(nll, transpose_result)]

extern crate atty;
extern crate env_logger;
#[macro_use]
extern crate failure;
extern crate getopts;
extern crate libc;
#[macro_use]
extern crate log;
extern crate nix;

macro_rules! display {
    ($fmt:expr) => (eprintln!(concat!(env!("CARGO_PKG_NAME"), ": ", $fmt)));
    ($fmt:expr, $($arg:tt)*) => (eprintln!(concat!(env!("CARGO_PKG_NAME"), ": ", $fmt), $($arg)*));
}

mod ast;
mod command;
mod cwd;
mod environment;
mod history;
mod interpreter;
mod lexer;
mod parser;
mod status;
mod word;

use std::env;
use std::fs::File;
use std::io;
use std::process;
use std::result;

use atty::Stream;
use env_logger::Builder;
use failure::ResultExt;
use getopts::Options;

use history::History;
use interpreter::Interpreter;

type Result<T> = result::Result<T, failure::Error>;

fn main() {
    Builder::from_default_env()
        .default_format_timestamp(false)
        .default_format_module_path(false)
        .init();

    let code = match run() {
        Ok(()) => 0,
        Err(e) => {
            print_error(&e);
            1
        }
    };

    process::exit(code);
}

fn run() -> Result<()> {
    let mut opts = Options::new();
    opts.optflag("V", "version", "Print version info and exit");
    opts.optflag("h", "help", "Display this message");

    let matches = opts.parse(env::args_os().skip(1)).unwrap_or_else(|e| {
        eprintln!("{}\n", e);
        print_usage_and_exit(&opts, 1);
    });

    if matches.opt_present("h") {
        print_usage_and_exit(&opts, 0);
    }

    if matches.opt_present("V") {
        println!(concat!(
            env!("CARGO_PKG_NAME"),
            " ",
            env!("CARGO_PKG_VERSION")
        ));
        return Ok(());
    }

    match matches.free.len() {
        0 => if atty::is(Stream::Stdin) {
            repl()
        } else {
            execute(io::stdin())
        },
        1 => {
            let path = matches.free[0].clone();
            if path == "-" {
                execute(io::stdin())
            } else {
                execute(File::open(&path).context(path)?)
            }
        }
        _ => {
            eprintln!("Only one 'FILE' argument is allowed\n");
            print_usage_and_exit(&opts, 1);
        }
    }
}

fn execute<R: io::Read>(mut reader: R) -> Result<()> {
    let mut src = Vec::new();
    reader.read_to_end(&mut src)?;

    let program = parser::parse(&src)?;
    Interpreter::new()?.execute(&program)
}

fn repl() -> Result<()> {
    let history = History::new()?;
    let mut interpreter = Interpreter::new()?;

    while let Some(line) = history.readline(&format!("{} $ ", interpreter.cwd()))? {
        if let Err(e) = parser::parse(&line).and_then(|stmts| interpreter.execute(&stmts)) {
            print_error(&e);
        }
    }

    Ok(())
}

fn print_usage_and_exit(opts: &Options, code: i32) -> ! {
    let usage = opts.usage(concat!("Usage: ", env!("CARGO_PKG_NAME"), " [FILE]"));
    if code == 0 {
        print!("{}", usage);
    } else {
        eprint!("{}", usage);
    }
    process::exit(code)
}

fn print_error(e: &failure::Error) {
    let causes: Vec<_> = e.causes().map(ToString::to_string).collect();
    display!("{}", causes.join(": "));
}
