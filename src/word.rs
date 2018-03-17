use std::ffi::{CStr, CString, OsStr, OsString};
use std::fmt;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

use libc;

#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub value: OsString,
    pub quote: Option<Quote>,
}

impl Word {
    pub fn new<B, Q>(buf: B, quote: Q) -> Self
    where
        B: Into<Vec<u8>>,
        Q: Into<Option<Quote>>,
    {
        Self {
            value: OsString::from_vec(buf.into()),
            quote: quote.into(),
        }
    }

    pub fn unquoted<B>(buf: B) -> Self
    where
        B: Into<Vec<u8>>,
    {
        Self::new(buf.into(), None)
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.value.as_bytes()
    }

    pub fn expand<P: AsRef<Path>>(&self, home: P) -> OsString {
        // TODO: expand environment variables.
        if self.quote.is_some() {
            return OsString::from_vec(self.as_bytes().to_vec());
        }

        let word = self.as_bytes();
        if !word.starts_with(b"~") {
            // No expansion necessary.
            return OsString::from_vec(word.into());
        }
        let word = &word[1..];

        let home = home.as_ref();
        if word.is_empty() {
            // ~
            return home.into();
        }

        if word.starts_with(b"/") {
            // ~/file
            return home.join(OsStr::from_bytes(&word[1..])).into_os_string();
        }

        // ~username[/rest]
        let (username, rest) = match word.iter().position(|b| *b == b'/') {
            Some(pos) => (&word[..pos], Some(&word[pos..])),
            None => (word, None),
        };

        home_directory(username)
            .map(|mut path| {
                if let Some(rest) = rest {
                    path.push(OsStr::from_bytes(rest));
                }
                path
            })
            .unwrap_or_else(|| {
                // User doesn't have a home directory. Return the word as-is.
                OsString::from_vec(self.as_bytes().into())
            })
    }
}

impl<'a> From<&'a str> for Word {
    fn from(value: &'a str) -> Word {
        Word::new(value.as_bytes().to_vec(), None)
    }
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let quote = match self.quote {
            Some(Quote::Single) => "'",
            Some(Quote::Double) => "\"",
            None => "",
        };
        write!(
            f,
            "{quote}{value}{quote}",
            value = self.value.to_string_lossy(),
            quote = quote
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Quote {
    Single,
    Double,
}

fn home_directory(username: &[u8]) -> Option<OsString> {
    let username = CString::new(username).unwrap();

    let c_str = unsafe {
        let result = libc::getpwnam(username.as_ptr());

        if result.is_null() || (*result).pw_dir.is_null() {
            return None;
        }

        CStr::from_ptr((*result).pw_dir)
    };

    Some(OsString::from_vec(c_str.to_bytes().to_vec()))
}

#[cfg(test)]
mod tests {
    use std::env;
    use super::*;

    fn home() -> OsString {
        env::var_os("HOME").unwrap()
    }

    fn user() -> OsString {
        env::var_os("USER").unwrap()
    }

    #[test]
    fn tilde_expansion() {
        assert_eq!(Word::unquoted("~").expand(&home()), home());
        assert_eq!(
            Word::unquoted("~/Desktop").expand(&home()),
            Path::new(&home()).join("Desktop"),
        );

        for quote in vec![Quote::Single, Quote::Double] {
            assert_eq!(Word::new("~", quote).expand(&home()), OsStr::new("~"));
        }
    }

    #[test]
    fn tilde_expansion_user() {
        let mut input = OsString::new();
        input.push("~");
        input.push(user());
        assert_eq!(Word::unquoted(input.as_bytes()).expand("unused"), home());
        input.push("/Downloads");
        assert_eq!(
            Word::unquoted(input.as_bytes()).expand("unused"),
            Path::new(&home()).join("Downloads"),
        );
    }
}
