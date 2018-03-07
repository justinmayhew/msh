use std::ffi::{CStr, CString};

use libc::{self, c_char, c_void};

pub fn readline(prompt: &str) -> Option<String> {
    let prompt = CString::new(prompt).expect("prompt has a null byte");
    unsafe {
        let value = ffi::readline(prompt.as_ptr() as *const c_char);
        if value.is_null() {
            return None;
        }

        let line = CStr::from_ptr(value)
            .to_str()
            .expect("invalid UTF-8")
            .to_owned();

        libc::free(value as *mut c_void);

        Some(line)
    }
}

mod ffi {
    use super::c_char;

    #[link(name = "readline")]
    extern "C" {
        pub fn readline(prompt: *const c_char) -> *mut c_char;
    }
}
