use std::borrow::Cow;
use std::ffi::{CStr, CString, OsStr, OsString};
use std::fmt;
use std::iter::Cloned;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::slice::Iter;

use libc;

use crate::ast::NameValuePair;
use crate::environment::Environment;
use crate::Result;

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

    pub fn to_os_string(&self) -> OsString {
        OsString::from_vec(self.as_bytes().to_vec())
    }

    pub fn as_os_str(&self) -> &OsStr {
        OsStr::from_bytes(self.as_bytes())
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.value.as_bytes()
    }

    pub fn is_valid_name(&self) -> bool {
        is_valid_name(self.as_bytes())
    }

    pub fn parse_name_value_pair(&self) -> Option<NameValuePair> {
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

        Some(NameValuePair::new(
            Word::unquoted(name),
            parse_quoted_word(value).unwrap_or_else(|| Word::unquoted(value)),
        ))
    }

    pub fn expand(&self, env: &Environment) -> Result<Cow<OsStr>> {
        match self.quote {
            Some(Quote::Single) => Ok(Cow::Borrowed(&self.value)),
            Some(Quote::Double) => expand_env_vars(Cow::Borrowed(&self.value), env),
            None => {
                let word = expand_tilde(&self.value, env.home());
                expand_env_vars(word, env)
            }
        }
    }
}

impl<'a> From<&'a str> for Word {
    fn from(value: &'a str) -> Word {
        Word::new(value.as_bytes().to_vec(), None)
    }
}

impl AsRef<OsStr> for Word {
    fn as_ref(&self) -> &OsStr {
        self.as_os_str()
    }
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{quote}{value}{quote}",
            value = self.value.to_string_lossy(),
            quote = if let Some(quote) = self.quote {
                quote.to_string()
            } else {
                "".to_string()
            }
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Quote {
    Single,
    Double,
}

impl fmt::Display for Quote {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match *self {
                Quote::Single => "'",
                Quote::Double => "\"",
            }
        )
    }
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

fn expand_tilde<H: AsRef<OsStr>>(word: &OsStr, home: H) -> Cow<OsStr> {
    let buf = word.as_bytes();
    if !buf.starts_with(b"~") {
        // No expansion necessary.
        return Cow::Borrowed(word);
    }
    let no_tilde = &buf[1..];

    let home = home.as_ref();
    if no_tilde.is_empty() {
        // ~
        return Cow::Owned(home.into());
    }

    if no_tilde.starts_with(b"/") {
        // ~/file
        let mut path = home.to_owned();
        path.push(OsStr::from_bytes(no_tilde));
        return Cow::Owned(path);
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
            Cow::Owned(path)
        })
        .unwrap_or_else(|| {
            // User doesn't have a home directory. Return the word as-is.
            Cow::Borrowed(word)
        })
}

fn expand_env_vars<'a>(word: Cow<'a, OsStr>, env: &Environment) -> Result<Cow<'a, OsStr>> {
    match word.as_bytes().iter().position(|&b| b == b'$') {
        Some(pos) => EnvExpander::new(word.as_bytes(), pos, env).expand(),
        None => Ok(word),
    }
}

struct EnvExpander<'a> {
    buf: Vec<u8>,
    bytes: Cloned<Iter<'a, u8>>,
    env: &'a Environment,
    peek: Option<u8>,
}

impl<'a> EnvExpander<'a> {
    fn new(word: &'a [u8], pos: usize, env: &'a Environment) -> Self {
        Self {
            buf: word[0..pos].to_vec(),
            bytes: word[pos + 1..].iter().cloned(),
            env,
            peek: None,
        }
    }

    fn expand<'word>(mut self) -> Result<Cow<'word, OsStr>> {
        // The starting position is the byte after the first $.
        self.expand_variable()?;

        while let Some(byte) = self.next_byte() {
            if byte == b'$' {
                self.expand_variable()?;
            } else {
                self.buf.push(byte);
            }
        }
        Ok(Cow::Owned(OsString::from_vec(self.buf)))
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
        if let Some(value) = self.env.get(OsStr::from_bytes(name)) {
            self.buf.extend(value.as_bytes());
        }
    }
}

fn is_valid_name(input: &[u8]) -> bool {
    !input.is_empty()
        && is_valid_first_byte(input[0])
        && input[1..].iter().cloned().all(is_valid_name_byte)
}

fn is_valid_first_byte(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_valid_name_byte(byte: u8) -> bool {
    is_valid_first_byte(byte) || byte.is_ascii_digit()
}

fn parse_quoted_word(value: &[u8]) -> Option<Word> {
    let len = value.len();
    if len < 2 {
        return None;
    }

    let (first, inner, last) = (value[0], &value[1..len - 1], value[len - 1]);
    if first != last {
        return None;
    }

    let quote = match first {
        b'\'' => Quote::Single,
        b'"' => Quote::Double,
        _ => return None,
    };
    Some(Word::new(inner, quote))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::Environment;
    use std::env;
    use std::path::Path;

    fn home() -> OsString {
        env::var_os("HOME").unwrap()
    }

    fn user() -> OsString {
        env::var_os("USER").unwrap()
    }

    #[test]
    fn tilde_expansion() {
        let env = Environment::new();
        assert_eq!(Word::unquoted("~").expand(&env).unwrap(), home());
        assert_eq!(
            Word::unquoted("~/Desktop").expand(&env).unwrap(),
            Path::new(&home()).join("Desktop"),
        );

        for quote in vec![Quote::Single, Quote::Double] {
            assert_eq!(Word::new("~", quote).expand(&env).unwrap(), OsStr::new("~"));
        }
    }

    #[test]
    fn tilde_expansion_user() {
        let env = Environment::new();
        let mut input = OsString::new();
        input.push("~");
        input.push(user());
        assert_eq!(
            Word::unquoted(input.as_bytes()).expand(&env).unwrap(),
            home()
        );
        input.push("/Downloads");
        assert_eq!(
            Word::unquoted(input.as_bytes()).expand(&env).unwrap(),
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

        let env = Environment::new();
        for (input, expected) in tests {
            for quote in vec![None, Some(Quote::Single), Some(Quote::Double)] {
                let word = Word::new(input.as_bytes(), quote);
                match quote {
                    None | Some(Quote::Double) => assert_eq!(word.expand(&env).unwrap(), expected),
                    Some(Quote::Single) => assert_eq!(word.expand(&env).unwrap(), input),
                }
            }
        }
    }

    #[test]
    fn name_value_pairs() {
        let word = Word::new("FOO=bar", None);
        assert_eq!(
            word.parse_name_value_pair(),
            Some(NameValuePair::new(
                Word::unquoted("FOO"),
                Word::unquoted("bar"),
            )),
        );

        for quote in vec![Quote::Single, Quote::Double] {
            let word = Word::new(format!("FOO={quote}bar{quote}", quote = quote), None);
            assert_eq!(
                word.parse_name_value_pair(),
                Some(NameValuePair::new(
                    Word::unquoted("FOO"),
                    Word::new("bar", quote),
                )),
            );
        }
    }
}
