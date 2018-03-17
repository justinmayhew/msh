use Result;
use ast::{Block, IfStmt, Program, Stmt, WhileStmt};
use command::Command;
use lexer::{Kind, Lexer, Token};
use word::Word;

pub fn parse(input: &[u8]) -> Result<Program> {
    Parser::new(input).parse()
}

struct Parser<'input> {
    lexer: Lexer<'input>,
    peek: Option<Token>,
}

macro_rules! expected {
    ($expected:expr, $found:expr) => ({
        let found = match $found {
            Some(token) => format!("{} on line {}", token.kind, token.line),
            None => "EOF".to_string(),
        };
        bail!("expected {}, found {}", $expected, found);
    });
}

impl<'input> Parser<'input> {
    fn new(src: &'input [u8]) -> Self {
        Self {
            lexer: Lexer::new(src),
            peek: None,
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>> {
        match self.peek.take() {
            Some(token) => Ok(Some(token)),
            None => match self.lexer.next() {
                Some(Ok(token)) => Ok(Some(token)),
                Some(Err(e)) => Err(e),
                None => Ok(None),
            },
        }
    }

    fn push_token(&mut self, token: Token) {
        assert!(self.peek.is_none());
        self.peek = Some(token);
    }

    fn match_token(&mut self, expected: &Kind) -> Result<bool> {
        match self.next_token()? {
            Some(next) => if next.kind == *expected {
                Ok(true)
            } else {
                self.push_token(next);
                Ok(false)
            },
            None => Ok(false),
        }
    }

    fn assert_token(&mut self, expected: &Kind) -> Result<()> {
        match self.next_token()? {
            Some(Token { ref kind, .. }) if kind == expected => Ok(()),
            token => expected!(expected, token),
        }
    }

    fn parse(mut self) -> Result<Program> {
        let mut program = Vec::new();

        while let Some(token) = self.next_token()? {
            let stmt = self.parse_stmt(token)?;
            self.assert_token(&Kind::Semi)?;
            program.push(stmt);
        }

        Ok(program)
    }

    fn parse_block(&mut self) -> Result<Block> {
        self.assert_token(&Kind::LeftBrace)?;

        let mut block = Vec::new();
        loop {
            match self.next_token()? {
                Some(ref token) if token.kind == Kind::RightBrace => break,
                Some(token) => {
                    let stmt = self.parse_stmt(token)?;
                    self.assert_token(&Kind::Semi)?;
                    block.push(stmt);
                }
                None => {
                    bail!("unexpected EOF parsing block");
                }
            }
        }

        Ok(block)
    }

    fn parse_stmt(&mut self, token: Token) -> Result<Stmt> {
        let word = assert_word(token, "statement")?;
        let stmt = if word.as_bytes() == b"if" {
            Stmt::If(self.parse_if_stmt()?)
        } else if word.as_bytes() == b"while" {
            Stmt::While(self.parse_while_stmt()?)
        } else {
            Stmt::Command(self.parse_command(Some(word))?)
        };
        Ok(stmt)
    }

    fn parse_if_stmt(&mut self) -> Result<IfStmt> {
        let test = self.parse_command(None)?;
        let consequent = self.parse_block()?;

        let alternate = if self.match_token(&Kind::Word("else".into()))? {
            Some(if self.match_token(&Kind::Word("if".into()))? {
                vec![Stmt::If(self.parse_if_stmt()?)]
            } else {
                self.parse_block()?
            })
        } else {
            None
        };

        Ok(IfStmt::new(test, consequent, alternate))
    }

    fn parse_while_stmt(&mut self) -> Result<WhileStmt> {
        let test = self.parse_command(None)?;
        let body = self.parse_block()?;
        Ok(WhileStmt::new(test, body))
    }

    fn parse_command(&mut self, mut name: Option<Word>) -> Result<Command> {
        let name = match name.take() {
            Some(name) => name,
            None => assert_word(self.next_token()?, "command")?,
        };
        let mut command = Command::from_name(name);

        loop {
            let token = self.next_token()?.unwrap();
            if let Kind::Word(argument) = token.kind {
                command.add_argument(argument);
            } else {
                self.push_token(token);
                break;
            }
        }

        Ok(command)
    }
}

fn assert_word<T>(token: T, expected: &str) -> Result<Word>
where
    T: Into<Option<Token>>,
{
    match token.into() {
        Some(Token {
            kind: Kind::Word(word),
            ..
        }) => Ok(word),
        token => expected!(expected, token),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        assert_eq!(
            parse(b"ls\n").unwrap(),
            vec![Stmt::Command(Command::from_name("ls".into()))],
        );
    }

    #[test]
    fn empty() {
        assert_eq!(parse(b"\n").unwrap(), Vec::new());
    }

    #[test]
    fn arguments() {
        assert_eq!(
            parse(b"cat /etc/hosts /etc/passwd\n").unwrap(),
            vec![
                Stmt::Command(Command::new(
                    "cat".into(),
                    vec!["/etc/hosts".into(), "/etc/passwd".into()],
                )),
            ],
        );
    }

    #[test]
    fn ignores_consecutive_spaces() {
        assert_eq!(
            parse(b"/bin/echo 1  2   3\n").unwrap(),
            vec![
                Stmt::Command(Command::new(
                    "/bin/echo".into(),
                    vec!["1".into(), "2".into(), "3".into()],
                )),
            ],
        );
    }

    #[test]
    fn ignores_leading_and_trailing_spaces() {
        assert_eq!(
            parse(b"  cat   \n").unwrap(),
            vec![Stmt::Command(Command::from_name("cat".into()))],
        );
    }

    #[test]
    fn semicolon_and_lf_ends_stmt() {
        assert_eq!(
            parse(b"echo 1; echo 2\necho 3\n").unwrap(),
            vec![
                Stmt::Command(Command::new("echo".into(), vec!["1".into()])),
                Stmt::Command(Command::new("echo".into(), vec!["2".into()])),
                Stmt::Command(Command::new("echo".into(), vec!["3".into()])),
            ],
        );
    }

    #[test]
    fn if_stmt() {
        assert_eq!(
            parse(b"if true { echo truthy }\n").unwrap(),
            vec![
                Stmt::If(IfStmt::new(
                    Command::from_name("true".into()),
                    vec![
                        Stmt::Command(Command::new("echo".into(), vec!["truthy".into()])),
                    ],
                    None,
                )),
            ],
        );
    }

    #[test]
    fn multiline_nested_if_else_stmt() {
        let src = br#"
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
        assert_eq!(
            parse(src).unwrap(),
            vec![
                Stmt::If(IfStmt::new(
                    Command::from_name("/bin/a".into()),
                    vec![Stmt::Command(Command::new("echo".into(), vec!["a".into()]))],
                    Some(vec![
                        Stmt::If(IfStmt::new(
                            Command::from_name("/bin/b".into()),
                            vec![
                                Stmt::Command(Command::new("echo".into(), vec!["b".into()])),
                                Stmt::Command(Command::new("echo".into(), vec!["2".into()])),
                                Stmt::If(IfStmt::new(
                                    Command::from_name("true".into()),
                                    vec![Stmt::Command(Command::from_name("exit".into()))],
                                    None,
                                )),
                            ],
                            Some(vec![
                                Stmt::Command(Command::new("echo".into(), vec!["c".into()])),
                            ]),
                        )),
                    ]),
                )),
            ],
        );
    }
}
