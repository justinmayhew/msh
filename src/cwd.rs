use std::env;
use std::path::{Path, PathBuf};

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

    pub fn cd(self, mut argv: Vec<String>) -> Result<Self> {
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

        Ok(Self {
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
