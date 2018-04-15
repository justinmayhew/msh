use std::fmt;
use std::fs::{File, OpenOptions};
use std::path::Path;

use failure::ResultExt;

use Result;
use word::Word;

#[derive(Clone, Debug, PartialEq)]
pub enum Redirect<P> {
    InFile(P),
    OutErr,
    OutFile(P, WriteMode),
    ErrOut,
    ErrFile(P, WriteMode),
}

impl fmt::Display for Redirect<Word> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Redirect::InFile(ref path) => write!(f, "<{}", path),
            Redirect::OutErr => write!(f, ">&2"),
            Redirect::OutFile(ref path, mode) => write!(f, "{}{}", mode, path),
            Redirect::ErrOut => write!(f, "2>&1"),
            Redirect::ErrFile(ref path, mode) => write!(f, "2{}{}", mode, path),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WriteMode {
    Truncate,
    Append,
}

impl WriteMode {
    pub fn open<P: AsRef<Path>>(&self, path: P) -> Result<File> {
        let mut file = OpenOptions::new();
        file.write(true).create(true);

        match *self {
            WriteMode::Truncate => {
                file.truncate(true);
            }
            WriteMode::Append => {
                file.append(true);
            }
        }

        let path = path.as_ref();
        Ok(file.open(path)
            .with_context(|_| path.display().to_string())?)
    }
}

impl fmt::Display for WriteMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            WriteMode::Truncate => write!(f, ">"),
            WriteMode::Append => write!(f, ">>"),
        }
    }
}
