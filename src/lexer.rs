use std::str::Chars;

pub struct Lexer<'input> {
    src: Chars<'input>,
    peek: Option<char>,
}

impl<'input> Lexer<'input> {
    pub fn new(src: &'input str) -> Self {
        Self {
            src: src.chars(),
            peek: None,
        }
    }

    fn next_char(&mut self) -> Option<char> {
        self.peek.take().or_else(|| self.src.next())
    }

    fn push_char(&mut self, c: char) {
        assert!(self.peek.is_none());
        self.peek = Some(c);
    }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = String::new();

        while let Some(c) = self.next_char() {
            if c == ';' || c == '\n' {
                if buf.is_empty() {
                    return Some(Token::Delimiter);
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
            None
        } else {
            Some(Token::Word(buf))
        }
    }
}

#[derive(Debug)]
pub enum Token {
    Word(String),
    Delimiter,
}
