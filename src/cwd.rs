use std::env;
use std::mem;
use std::path::{Path, PathBuf};

use status::Status;

pub struct Cwd {
    path: PathBuf,
    last: Option<PathBuf>,
}

impl Cwd {
    pub fn new() -> Self {
        Self {
            path: env::current_dir().unwrap(),
            last: None,
        }
    }

    pub fn current(&self) -> &Path {
        &self.path
    }

    pub fn cd(&mut self, argv: &[String]) -> Status {
        if argv.len() > 1 {
            display!("cd: too many arguments");
            return Status::Failure;
        }

        let path = match argv.first() {
            Some(path) => {
                if path == "-" {
                    self.last.as_ref().unwrap_or(&self.path).clone()
                } else {
                    str_to_pathbuf(path)
                }
            }
            None => env::home_dir().expect("HOME required"),
        };

        if let Err(e) = env::set_current_dir(&path) {
            display!("cd: can't cd to {}: {}", path.display(), e);
            return Status::Failure;
        }

        let absolute = if path.is_relative() {
            self.path.join(path)
        } else {
            path
        };

        self.last = Some(mem::replace(
            &mut self.path,
            absolute.canonicalize().expect("error canonicalizing path"),
        ));

        Status::Success
    }
}

fn str_to_pathbuf(s: &str) -> PathBuf {
    let mut buf = PathBuf::new();
    buf.push(s);
    buf
}
