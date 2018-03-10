use std::env;
use std::mem;
use std::path::{Path, PathBuf};

use failure::ResultExt;

use Result;

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

    pub fn cd(&mut self, mut argv: Vec<String>) -> Result<()> {
        assert!(argv.len() <= 1);

        let path = match argv.pop() {
            Some(path) => {
                if path == "-" {
                    self.last.as_ref().unwrap_or(&self.path).clone()
                } else {
                    str_to_pathbuf(&path)
                }
            }
            None => env::home_dir().expect("HOME required"),
        };

        env::set_current_dir(&path).with_context(|_| path.display().to_string())?;

        let absolute = if path.is_relative() {
            self.path.join(path)
        } else {
            path
        };

        self.last = Some(mem::replace(
            &mut self.path,
            absolute.canonicalize().expect("error canonicalizing path"),
        ));

        Ok(())
    }
}

fn str_to_pathbuf(s: &str) -> PathBuf {
    let mut buf = PathBuf::new();
    buf.push(s);
    buf
}
