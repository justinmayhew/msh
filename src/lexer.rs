use std::fmt;
use std::slice::Iter;

use Result;
use word::{Quote, Word};
use redirect::{Redirect, WriteMode};

pub struct Lexer<'input> {
    src: Iter<'input, u8>,
    line: usize,
    peek: Option<u8>,
    next: Option<Kind>,
    last: Option<Kind>,
}

impl<'input> Lexer<'input> {
    pub fn new(src: &'input [u8]) -> Self {
        Self {
            src: src.iter(),
            line: 1,
            peek: None,
            next: None,
            last: None,
        }
    }

    fn emit(&mut self, kind: Kind, line: Option<usize>) -> Option<Result<Token>> {
        self.last = Some(kind.clone());
        Some(Ok(Token::new(kind, line.unwrap_or(self.line))))
    }

    fn next_byte(&mut self) -> Option<u8> {
        let next = self.peek.take().or_else(|| self.src.next().cloned());
        if next == Some(b'\n') {
            self.line += 1;
        }
        next
    }

    fn push_byte(&mut self, byte: u8) {
        assert!(self.peek.is_none());
        if byte == b'\n' {
            self.line -= 1;
        }
        self.peek = Some(byte);
    }

    fn consume_line_terminators(&mut self) {
        while let Some(c) = self.next_byte() {
            if !is_line_terminator(c) {
                self.push_byte(c);
                break;
            }
        }
    }

    fn consume_quoted_word(&mut self, quote: u8) -> Option<Result<Token>> {
        let line = self.line;
        let mut buf = Vec::new();

        while let Some(byte) = self.next_byte() {
            if byte == quote {
                let quote = if quote == b'"' {
                    Quote::Double
                } else {
                    Quote::Single
                };
                return self.emit(Kind::Word(Word::new(buf, quote)), Some(line));
            }
            buf.push(byte)
        }

        Some(Err(format_err!(
            "missing closing quote{}",
            if buf.is_empty() {
                "".into()
            } else {
                format!(" for: {}", String::from_utf8_lossy(&buf))
            }
        )))
    }

    fn consume_redirect(&mut self, fd: Stream) -> Result<(Redirect<Word>, usize)> {
        let line = self.line;

        let mode = if fd.is_writable() {
            match self.next_byte() {
                Some(b'>') => WriteMode::Append,
                Some(byte) => {
                    self.push_byte(byte);
                    WriteMode::Truncate
                }
                None => WriteMode::Truncate,
            }
        } else {
            WriteMode::Truncate
        };

        let location = self.consume_redirect_location()?;

        if let Location::Stream(stream) = location {
            if mode == WriteMode::Append {
                bail!("cannot open {} in append mode", stream);
            }
        }

        let redirect = match fd {
            Stream::Stdin => match location {
                Location::Path(path) => Redirect::InFile(path),
                Location::Stream(stream) => bail!("cannot redirect stdin to {}", stream),
            },
            Stream::Stdout => match location {
                Location::Path(path) => Redirect::OutFile(path, mode),
                Location::Stream(WritableStream::Stdout) => {
                    bail!("cannot redirect stdout to itself")
                }
                Location::Stream(WritableStream::Stderr) => Redirect::OutErr,
            },
            Stream::Stderr => match location {
                Location::Path(path) => Redirect::ErrFile(path, mode),
                Location::Stream(WritableStream::Stdout) => Redirect::ErrOut,
                Location::Stream(WritableStream::Stderr) => {
                    bail!("cannot redirect stderr to itself")
                }
            },
        };
        Ok((redirect, line))
    }

    fn consume_redirect_location(&mut self) -> Result<Location> {
        let location = match self.next_byte() {
            Some(b'&') => match self.next_byte() {
                Some(b'1') => Location::Stream(WritableStream::Stdout),
                Some(b'2') => Location::Stream(WritableStream::Stderr),
                Some(byte) => bail!(
                    "expected 1 (stdout) or 2 (stderr), found '{}'",
                    byte as char
                ),
                None => bail!("expected 1 (stdout) or 2 (stderr)"),
            },
            Some(other) => {
                self.push_byte(other);
                let token = self.next().expect("expected token")?;
                if let Kind::Word(word) = token.kind {
                    Location::Path(word)
                } else {
                    bail!("expected redirect location, found {}", token.kind);
                }
            }
            None => bail!("expected redirect location"),
        };
        Ok(location)
    }

    fn should_insert_semi(&self) -> bool {
        match self.last {
            Some(ref kind) => *kind != Kind::LeftBrace && *kind != Kind::Semi,
            None => false,
        }
    }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Result<Token>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(kind) = self.next.take() {
            return self.emit(kind, None);
        }

        let mut buf = Vec::new();

        while let Some(byte) = self.next_byte() {
            if buf.is_empty() {
                match byte {
                    b'"' | b'\'' => return self.consume_quoted_word(byte),
                    b'{' => {
                        let line = self.line;
                        self.consume_line_terminators();
                        return self.emit(Kind::LeftBrace, Some(line));
                    }
                    b'}' => {
                        let kind = if self.should_insert_semi() {
                            self.next = Some(Kind::RightBrace);
                            Kind::Semi
                        } else {
                            Kind::RightBrace
                        };
                        return self.emit(kind, None);
                    }
                    b'|' => return self.emit(Kind::Pipe, None),
                    b @ b'>' | b @ b'<' => {
                        return match self.consume_redirect(if b == b'>' {
                            Stream::Stdout
                        } else {
                            Stream::Stdin
                        }) {
                            Ok((redirect, line)) => self.emit(Kind::Redirect(redirect), Some(line)),
                            Err(e) => Some(Err(e)),
                        };
                    }
                    fd @ b'0' => match self.next_byte() {
                        Some(b'<') => {
                            return match self.consume_redirect(Stream::Stdin) {
                                Ok((redirect, line)) => {
                                    self.emit(Kind::Redirect(redirect), Some(line))
                                }
                                Err(e) => Some(Err(e)),
                            };
                        }
                        Some(other) => {
                            buf.push(fd);
                            self.push_byte(other);
                            continue;
                        }
                        None => {
                            buf.push(fd);
                            break;
                        }
                    },
                    fd @ b'1' | fd @ b'2' => match self.next_byte() {
                        Some(b'>') => {
                            return match self.consume_redirect(if fd == b'1' {
                                Stream::Stdout
                            } else {
                                Stream::Stderr
                            }) {
                                Ok((redirect, line)) => {
                                    self.emit(Kind::Redirect(redirect), Some(line))
                                }
                                Err(e) => Some(Err(e)),
                            };
                        }
                        Some(other) => {
                            buf.push(fd);
                            self.push_byte(other);
                            continue;
                        }
                        None => {
                            buf.push(fd);
                            break;
                        }
                    },
                    _ => {}
                }
            }

            if is_line_terminator(byte) {
                if buf.is_empty() {
                    let line = self.line;
                    self.consume_line_terminators();
                    return if self.last.is_none() {
                        // Don't emit leading delimiters.
                        self.next()
                    } else {
                        self.emit(Kind::Semi, Some(line - 1))
                    };
                } else {
                    self.push_byte(byte);
                    break;
                }
            }

            if byte.is_ascii_whitespace() {
                if buf.is_empty() {
                    // Ignore consecutive whitespace.
                    continue;
                } else {
                    // At the end of a token.
                    break;
                }
            }

            buf.push(byte);
        }

        if buf.is_empty() {
            match self.last {
                Some(Kind::Semi) | None => None,
                Some(_) => {
                    // Emit a trailing semi to reduce edge cases in the parser.
                    self.emit(Kind::Semi, None)
                }
            }
        } else {
            self.emit(Kind::Word(Word::unquoted(buf)), None)
        }
    }
}

fn is_line_terminator(byte: u8) -> bool {
    byte == b'\n' || byte == b';'
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: Kind,
    pub line: usize,
}

impl Token {
    fn new(kind: Kind, line: usize) -> Self {
        Self { kind, line }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Word(Word),
    Redirect(Redirect<Word>),
    LeftBrace,
    RightBrace,
    Pipe,
    Semi,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match *self {
            Kind::Word(ref word) => word.to_string(),
            Kind::Redirect(ref redirect) => redirect.to_string(),
            Kind::LeftBrace => "{".into(),
            Kind::RightBrace => "}".into(),
            Kind::Pipe => "|".into(),
            Kind::Semi => ";".into(),
        };

        write!(f, "'{}'", s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stream {
    Stdin,
    Stdout,
    Stderr,
}

impl Stream {
    fn is_writable(&self) -> bool {
        match *self {
            Stream::Stdin => false,
            Stream::Stdout | Stream::Stderr => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Location {
    Path(Word),
    Stream(WritableStream),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum WritableStream {
    Stdout,
    Stderr,
}

impl fmt::Display for WritableStream {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            WritableStream::Stdout => write!(f, "stdout"),
            WritableStream::Stderr => write!(f, "stderr"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command() {
        let tokens: Vec<Kind> = Lexer::new(b"cat /etc/hosts /etc/passwd")
            .map(|t| t.unwrap().kind)
            .collect();
        assert_eq!(
            tokens,
            vec![
                Kind::Word("cat".into()),
                Kind::Word("/etc/hosts".into()),
                Kind::Word("/etc/passwd".into()),
                Kind::Semi,
            ],
        );
    }

    #[test]
    fn empty() {
        let tokens: Vec<Kind> = Lexer::new(b"\n").map(|t| t.unwrap().kind).collect();
        assert_eq!(tokens, Vec::new());
    }

    #[test]
    fn double_quotes() {
        let tokens: Vec<Kind> = Lexer::new(br#"echo "I'm quoted""#)
            .map(|t| t.unwrap().kind)
            .collect();
        assert_eq!(
            tokens,
            vec![
                Kind::Word("echo".into()),
                Kind::Word(Word::new("I'm quoted", Quote::Double)),
                Kind::Semi,
            ]
        );
    }

    #[test]
    fn double_quotes_unclosed() {
        let mut lexer = Lexer::new(br#"echo "Missing closing quote"#);
        assert_eq!(
            lexer.next().unwrap().unwrap().kind,
            Kind::Word("echo".into())
        );
        assert!(lexer.next().unwrap().is_err());
    }

    #[test]
    fn if_stmt() {
        let tokens: Vec<Kind> = Lexer::new(b"if true { echo truthy }\n")
            .map(|t| t.unwrap().kind)
            .collect();
        assert_eq!(
            tokens,
            vec![
                Kind::Word("if".into()),
                Kind::Word("true".into()),
                Kind::LeftBrace,
                Kind::Word("echo".into()),
                Kind::Word("truthy".into()),
                Kind::Semi,
                Kind::RightBrace,
                Kind::Semi,
            ],
        );
    }

    #[test]
    fn empty_body() {
        let tokens: Vec<Kind> = Lexer::new(b"if false { }\n")
            .map(|t| t.unwrap().kind)
            .collect();
        assert_eq!(
            tokens,
            vec![
                Kind::Word("if".into()),
                Kind::Word("false".into()),
                Kind::LeftBrace,
                Kind::RightBrace,
                Kind::Semi,
            ],
        );
    }

    #[test]
    fn multiline_nested_if_else_stmt() {
        let src = br#"if /bin/a {
  echo a
} else if /bin/b {
  echo b
  echo 2
  if true {
    exit
  }
} else {
  echo c
}
"#;
        let tokens: Vec<Token> = Lexer::new(src).map(|t| t.unwrap()).collect();
        assert_eq!(
            tokens,
            vec![
                Token::new(Kind::Word("if".into()), 1),
                Token::new(Kind::Word("/bin/a".into()), 1),
                Token::new(Kind::LeftBrace, 1),
                Token::new(Kind::Word("echo".into()), 2),
                Token::new(Kind::Word("a".into()), 2),
                Token::new(Kind::Semi, 2),
                Token::new(Kind::RightBrace, 3),
                Token::new(Kind::Word("else".into()), 3),
                Token::new(Kind::Word("if".into()), 3),
                Token::new(Kind::Word("/bin/b".into()), 3),
                Token::new(Kind::LeftBrace, 3),
                Token::new(Kind::Word("echo".into()), 4),
                Token::new(Kind::Word("b".into()), 4),
                Token::new(Kind::Semi, 4),
                Token::new(Kind::Word("echo".into()), 5),
                Token::new(Kind::Word("2".into()), 5),
                Token::new(Kind::Semi, 5),
                Token::new(Kind::Word("if".into()), 6),
                Token::new(Kind::Word("true".into()), 6),
                Token::new(Kind::LeftBrace, 6),
                Token::new(Kind::Word("exit".into()), 7),
                Token::new(Kind::Semi, 7),
                Token::new(Kind::RightBrace, 8),
                Token::new(Kind::Semi, 8),
                Token::new(Kind::RightBrace, 9),
                Token::new(Kind::Word("else".into()), 9),
                Token::new(Kind::LeftBrace, 9),
                Token::new(Kind::Word("echo".into()), 10),
                Token::new(Kind::Word("c".into()), 10),
                Token::new(Kind::Semi, 10),
                Token::new(Kind::RightBrace, 11),
                Token::new(Kind::Semi, 11),
            ],
        );
    }

    #[test]
    fn pipeline() {
        let tokens: Vec<Kind> = Lexer::new(b"echo foo | cat\n")
            .map(|t| t.unwrap().kind)
            .collect();
        assert_eq!(
            tokens,
            vec![
                Kind::Word("echo".into()),
                Kind::Word("foo".into()),
                Kind::Pipe,
                Kind::Word("cat".into()),
                Kind::Semi,
            ],
        );
    }
}
