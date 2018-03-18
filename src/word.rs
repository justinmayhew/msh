use std::env;
use std::ffi::{CStr, CString, OsStr, OsString};
use std::fmt;
use std::iter::Cloned;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::slice::Iter;

use libc;

use Result;

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

    pub fn parse_name_value_pair(&self) -> Option<(OsString, OsString)> {
        if self.quote.is_some() {
            return None;
        }

        let word = self.as_bytes();
        let (name, value) = match word.iter().position(|&b| b == b'=') {
            Some(0) | None => return None,
            Some(pos) => (&word[..pos], &word[pos + 1..]),
        };

        if !is_valid_name(name) {
            return None;
        }

        Some((
            OsString::from_vec(name.to_vec()),
            OsString::from_vec(value.to_vec()),
        ))
    }

    pub fn expand<H: AsRef<OsStr>>(&self, home: H) -> Result<OsString> {
        let word = self.as_bytes();

        match self.quote {
            Some(Quote::Single) => Ok(OsString::from_vec(word.to_vec())),
            Some(Quote::Double) => expand_env_vars(word),
            None => {
                let word = expand_tilde(word, home);
                expand_env_vars(word.as_bytes())
            }
        }
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

fn expand_tilde<H: AsRef<OsStr>>(word: &[u8], home: H) -> OsString {
    if !word.starts_with(b"~") {
        // No expansion necessary.
        return OsString::from_vec(word.into());
    }
    let no_tilde = &word[1..];

    let home = home.as_ref();
    if no_tilde.is_empty() {
        // ~
        return home.into();
    }

    if no_tilde.starts_with(b"/") {
        // ~/file
        let mut path = home.to_owned();
        path.push(OsStr::from_bytes(no_tilde));
        return path;
    }

    // ~username[/rest]
    let (username, rest) = match no_tilde.iter().position(|b| *b == b'/') {
        Some(pos) => (&no_tilde[..pos], Some(&no_tilde[pos..])),
        None => (no_tilde, None),
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
            OsString::from_vec(word.into())
        })
}

fn expand_env_vars(word: &[u8]) -> Result<OsString> {
    match word.iter().position(|&b| b == b'$') {
        Some(pos) => EnvExpander::new(word, pos).expand(),
        None => Ok(OsString::from_vec(word.to_vec())),
    }
}

struct EnvExpander<'a> {
    buf: Vec<u8>,
    bytes: Cloned<Iter<'a, u8>>,
    peek: Option<u8>,
}

impl<'a> EnvExpander<'a> {
    fn new(word: &'a [u8], pos: usize) -> Self {
        Self {
            buf: word[0..pos].to_vec(),
            bytes: word[pos + 1..].iter().cloned(),
            peek: None,
        }
    }

    fn expand(mut self) -> Result<OsString> {
        // The starting position is the byte after the first $.
        self.expand_variable()?;

        while let Some(byte) = self.next_byte() {
            if byte == b'$' {
                self.expand_variable()?;
            } else {
                self.buf.push(byte);
            }
        }
        Ok(OsString::from_vec(self.buf))
    }

    fn next_byte(&mut self) -> Option<u8> {
        self.peek.take().or_else(|| self.bytes.next())
    }

    fn push_byte(&mut self, byte: u8) {
        assert!(self.peek.is_none());
        self.peek = Some(byte);
    }

    fn expand_variable(&mut self) -> Result<()> {
        if let Some(byte) = self.next_byte() {
            let mut name = Vec::new();
            if byte == b'{' {
                if !self.consume_while(&mut name, |b| b != b'}', false) {
                    bail!(
                        "missing variable closing brace{}",
                        if name.is_empty() {
                            "".into()
                        } else {
                            format!(" around: {}", String::from_utf8_lossy(&name))
                        }
                    );
                }
            } else {
                self.push_byte(byte);
                self.consume_while(&mut name, is_valid_name_byte, true);
            }
            if !is_valid_name(&name) {
                bail!("invalid variable name: {}", String::from_utf8_lossy(&name));
            }
            self.append_var(&name);
        } else {
            self.buf.push(b'$');
        }
        Ok(())
    }

    fn consume_while<F>(&mut self, buf: &mut Vec<u8>, predicate: F, keep_last: bool) -> bool
    where
        F: Fn(u8) -> bool,
    {
        while let Some(byte) = self.next_byte() {
            if predicate(byte) {
                buf.push(byte);
            } else {
                if keep_last {
                    self.push_byte(byte);
                }
                return true;
            }
        }
        false
    }

    fn append_var(&mut self, name: &[u8]) {
        if let Some(value) = env::var_os(OsStr::from_bytes(name)) {
            self.buf.extend(value.as_bytes());
        }
    }
}

fn is_valid_name(input: &[u8]) -> bool {
    !input.is_empty() && is_valid_first_byte(input[0])
        && input[1..].iter().cloned().all(is_valid_name_byte)
}

fn is_valid_first_byte(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_valid_name_byte(byte: u8) -> bool {
    is_valid_first_byte(byte) || byte.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::path::Path;
    use super::*;

    fn home() -> OsString {
        env::var_os("HOME").unwrap()
    }

    fn user() -> OsString {
        env::var_os("USER").unwrap()
    }

    #[test]
    fn tilde_expansion() {
        assert_eq!(Word::unquoted("~").expand(&home()).unwrap(), home());
        assert_eq!(
            Word::unquoted("~/Desktop").expand(&home()).unwrap(),
            Path::new(&home()).join("Desktop"),
        );

        for quote in vec![Quote::Single, Quote::Double] {
            assert_eq!(
                Word::new("~", quote).expand(&home()).unwrap(),
                OsStr::new("~")
            );
        }
    }

    #[test]
    fn tilde_expansion_user() {
        let mut input = OsString::new();
        input.push("~");
        input.push(user());
        assert_eq!(Word::unquoted(input.as_bytes()).expand("").unwrap(), home());
        input.push("/Downloads");
        assert_eq!(
            Word::unquoted(input.as_bytes()).expand("").unwrap(),
            Path::new(&home()).join("Downloads"),
        );
    }

    #[test]
    fn env_var_expansion() {
        let vars = vec![("FOO", "bar"), ("ONE", "1"), ("HOST", "www.example.com")];
        for &(name, value) in &vars {
            env::set_var(name, value);
        }

        let mut tests = Vec::new();
        for &(name, value) in &vars {
            tests.push((
                OsString::from_vec(format!("${name}", name = name).into_bytes()),
                OsStr::new(value),
            ));
            tests.push((
                OsString::from_vec(format!("${{{name}}}", name = name).into_bytes()),
                OsStr::new(value),
            ));
        }

        for (input, expected) in tests {
            for quote in vec![None, Some(Quote::Single), Some(Quote::Double)] {
                let word = Word::new(input.as_bytes(), quote);
                match quote {
                    None | Some(Quote::Double) => assert_eq!(word.expand("").unwrap(), expected),
                    Some(Quote::Single) => assert_eq!(word.expand("").unwrap(), input),
                }
            }
        }
    }
}
