use std::env;
use std::ffi::{CStr, CString};
use std::fs::OpenOptions;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use libc::{self, c_char, c_int, c_void};

use Result;

pub struct History {
    path: CString,
}

impl History {
    pub fn new(history_path: Option<&PathBuf>) -> Result<Self> {
        let history_path = history_path
            .map(Into::into)
            .unwrap_or_else(|| env::home_dir().expect("HOME required").join(".msh_history"));

        let path = CString::new(history_path.as_os_str().as_bytes())?;

        if let Err(e) = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&history_path)
        {
            if e.kind() != io::ErrorKind::AlreadyExists {
                bail!("error creating history file {:?}: {}", history_path, e);
            }
        }

        unsafe {
            if ffi::read_history(path.as_ptr()) != 0 {
                bail!("error loading history file {:?}", history_path);
            }
        }

        Ok(Self { path })
    }

    pub fn readline(&self, prompt: &str) -> Result<Option<Vec<u8>>> {
        let prompt = CString::new(prompt)?;
        unsafe {
            let value = ffi::readline(prompt.as_ptr() as *const c_char);
            if value.is_null() {
                return Ok(None);
            }

            let line = CStr::from_ptr(value).to_bytes().to_owned();

            if !line.is_empty() {
                ffi::add_history(value);
                if ffi::append_history(1, self.path.as_ptr()) != 0 {
                    bail!("failed writing history");
                }
            }

            libc::free(value as *mut c_void);

            Ok(Some(line))
        }
    }
}

mod ffi {
    use super::*;

    #[link(name = "readline")]
    extern "C" {
        pub fn read_history(filename: *const c_char) -> c_int;
        pub fn append_history(nelements: c_int, filename: *const c_char) -> c_int;

        pub fn readline(prompt: *const c_char) -> *mut c_char;
        pub fn add_history(line: *const c_char);
    }
}
