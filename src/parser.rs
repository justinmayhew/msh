use Result;
use ast::{Block, Exportable, IfStmt, Program, Stmt, WhileStmt};
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
            None => self.lexer.next().transpose(),
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

    fn match_word(&mut self) -> Result<Option<Word>> {
        match self.next_token()? {
            Some(next) => if let Kind::Word(word) = next.kind {
                Ok(Some(word))
            } else {
                self.push_token(next);
                Ok(None)
            },
            None => Ok(None),
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
        while let Some(token) = self.next_token()? {
            if token.kind == Kind::RightBrace {
                return Ok(block);
            }

            block.push(self.parse_stmt(token)?);
            self.assert_token(&Kind::Semi)?;
        }

        bail!("unexpected EOF parsing block");
    }

    fn parse_stmt(&mut self, token: Token) -> Result<Stmt> {
        let word = assert_word(token, "statement")?;
        Ok(match word.as_bytes() {
            b"if" => Stmt::If(self.parse_if_stmt()?),
            b"while" => Stmt::While(self.parse_while_stmt()?),
            b"export" => Stmt::Export(self.parse_export_stmt()?),
            _ => self.parse_assignment_or_command(word)?,
        })
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

    fn parse_export_stmt(&mut self) -> Result<Vec<Exportable>> {
        let mut exports = Vec::new();
        while let Some(word) = self.match_word()? {
            if let Some(pair) = word.parse_name_value_pair() {
                exports.push(Exportable::new(pair.name, Some(pair.value)));
            } else if word.is_valid_name() {
                exports.push(Exportable::new(word, None));
            } else {
                bail!("not a valid name: {}", word);
            }
        }

        if exports.is_empty() {
            bail!("expected at least one name or name value pair after export");
        } else {
            Ok(exports)
        }
    }

    fn parse_assignment_or_command(&mut self, word: Word) -> Result<Stmt> {
        if let Some(pair) = word.parse_name_value_pair() {
            let mut env = vec![pair];
            while let Some(word) = self.match_word()? {
                if let Some(pair) = word.parse_name_value_pair() {
                    env.push(pair);
                } else {
                    let mut command = self.parse_command(Some(word))?;
                    command.set_env(env);
                    return Ok(Stmt::Command(command));
                }
            }
            Ok(Stmt::Assignment(env))
        } else {
            Ok(Stmt::Command(self.parse_command(Some(word))?))
        }
    }

    fn parse_command(&mut self, mut name: Option<Word>) -> Result<Command> {
        let name = match name.take() {
            Some(name) => name,
            None => assert_word(self.next_token()?, "command")?,
        };
        let mut command = Command::from_name(name);

        while let Some(argument) = self.match_word()? {
            command.add_argument(argument);
        }

        if self.match_token(&Kind::Pipe)? {
            command.set_pipeline(self.parse_command(None)?);
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
    use ast::NameValuePair;

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
    fn assignment() {
        assert_eq!(
            parse(b"FOO=bar one=ONE").unwrap(),
            vec![
                Stmt::Assignment(vec![
                    NameValuePair::new(Word::unquoted("FOO"), Word::unquoted("bar")),
                    NameValuePair::new(Word::unquoted("one"), Word::unquoted("ONE")),
                ]),
            ],
        );
    }

    #[test]
    fn command_with_assignment() {
        let mut command = Command::new("./server".into(), vec!["--http".into()]);
        command.set_env(vec![
            NameValuePair::new(Word::unquoted("PORT"), Word::unquoted("8000")),
        ]);
        assert_eq!(
            parse(b"PORT=8000 ./server --http").unwrap(),
            vec![Stmt::Command(command)]
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

    #[test]
    fn pipeline() {
        let mut cmd = Command::new("echo".into(), vec!["Hello".into(), "world".into()]);
        cmd.set_pipeline(Command::new("rg".into(), vec!["world".into()]));
        assert_eq!(
            parse(b"echo Hello world | rg world\n").unwrap(),
            vec![Stmt::Command(cmd)],
        );
    }
}
