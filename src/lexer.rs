use std::str::Chars;

pub struct Lexer<'input> {
    src: Chars<'input>,
    peek: Option<char>,
    next: Option<Token>,
    last: Option<Token>,
    at_start: bool,
}

impl<'input> Lexer<'input> {
    pub fn new(src: &'input str) -> Self {
        Self {
            src: src.chars(),
            peek: None,
            next: None,
            last: None,
            at_start: true,
        }
    }

    fn emit(&mut self, token: Token) -> Option<Token> {
        self.at_start = false;
        self.last = Some(token.clone());
        Some(token)
    }

    fn next_char(&mut self) -> Option<char> {
        self.peek.take().or_else(|| self.src.next())
    }

    fn push_char(&mut self, c: char) {
        assert!(self.peek.is_none());
        self.peek = Some(c);
    }

    fn consume_line_terminators(&mut self) {
        while let Some(c) = self.next_char() {
            if !is_line_terminator(c) {
                self.push_char(c);
                break;
            }
        }
    }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(token) = self.next.take() {
            return self.emit(token);
        }

        let mut buf = String::new();

        while let Some(c) = self.next_char() {
            if buf.is_empty() {
                if c == '{' {
                    self.consume_line_terminators();
                    return self.emit(Token::LeftBrace);
                }
                if c == '}' {
                    if self.last != Some(Token::Semi) {
                        self.next = Some(Token::RightBrace);
                        return self.emit(Token::Semi);
                    } else {
                        return self.emit(Token::RightBrace);
                    }
                }
            }

            if is_line_terminator(c) {
                if buf.is_empty() {
                    self.consume_line_terminators();
                    return if self.at_start {
                        // Don't emit leading delimiters.
                        self.next()
                    } else {
                        self.emit(Token::Semi)
                    };
                } else {
                    self.push_char(c);
                    break;
                }
            }

            if c.is_whitespace() {
                if buf.is_empty() {
                    // Ignore consecutive whitespace.
                    continue;
                } else {
                    // At the end of a token.
                    break;
                }
            }

            buf.push(c);
        }

        if buf.is_empty() {
            // Ensure we always emit a trailing semi to reduce
            // edge cases in the parser.
            if self.last == Some(Token::Semi) {
                None
            } else {
                self.emit(Token::Semi)
            }
        } else {
            self.emit(Token::Word(buf))
        }
    }
}

fn is_line_terminator(c: char) -> bool {
    c == '\n' || c == ';'
}

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Word(String),
    LeftBrace,
    RightBrace,
    Semi,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command() {
        let tokens: Vec<Token> = Lexer::new("cat /etc/hosts /etc/passwd").collect();
        assert_eq!(
            tokens,
            vec![
                Token::Word("cat".into()),
                Token::Word("/etc/hosts".into()),
                Token::Word("/etc/passwd".into()),
                Token::Semi,
            ],
        );
    }

    #[test]
    fn if_stmt() {
        let tokens: Vec<Token> = Lexer::new("if true { echo truthy }\n").collect();
        assert_eq!(
            tokens,
            vec![
                Token::Word("if".into()),
                Token::Word("true".into()),
                Token::LeftBrace,
                Token::Word("echo".into()),
                Token::Word("truthy".into()),
                Token::Semi,
                Token::RightBrace,
                Token::Semi,
            ],
        );
    }

    #[test]
    fn multiline_nested_if_else_stmt() {
        let src = r#"
if /bin/a {
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
        let tokens: Vec<Token> = Lexer::new(src).collect();
        assert_eq!(
            tokens,
            vec![
                Token::Word("if".into()),
                Token::Word("/bin/a".into()),
                Token::LeftBrace,
                Token::Word("echo".into()),
                Token::Word("a".into()),
                Token::Semi,
                Token::RightBrace,
                Token::Word("else".into()),
                Token::Word("if".into()),
                Token::Word("/bin/b".into()),
                Token::LeftBrace,
                Token::Word("echo".into()),
                Token::Word("b".into()),
                Token::Semi,
                Token::Word("echo".into()),
                Token::Word("2".into()),
                Token::Semi,
                Token::Word("if".into()),
                Token::Word("true".into()),
                Token::LeftBrace,
                Token::Word("exit".into()),
                Token::Semi,
                Token::RightBrace,
                Token::Semi,
                Token::RightBrace,
                Token::Word("else".into()),
                Token::LeftBrace,
                Token::Word("echo".into()),
                Token::Word("c".into()),
                Token::Semi,
                Token::RightBrace,
                Token::Semi,
            ],
        );
    }
}
