use Result;
use ast::{Block, IfStmt, Program, Stmt, WhileStmt};
use command::Command;
use lexer::{Lexer, Token};

pub fn parse(input: &str) -> Result<Program> {
    Parser::new(input).parse()
}

struct Parser<'input> {
    lexer: Lexer<'input>,
    peek: Option<Token>,
}

impl<'input> Parser<'input> {
    fn new(src: &'input str) -> Self {
        Self {
            lexer: Lexer::new(src),
            peek: None,
        }
    }

    fn next_token(&mut self) -> Option<Token> {
        self.peek.take().or_else(|| self.lexer.next())
    }

    fn push_token(&mut self, token: Token) {
        assert!(self.peek.is_none());
        self.peek = Some(token);
    }

    fn match_token(&mut self, token: &Token) -> bool {
        match self.next_token() {
            Some(next) => if next == *token {
                true
            } else {
                self.push_token(next);
                false
            },
            None => false,
        }
    }

    fn assert_token(&mut self, token: &Token) -> Result<()> {
        match self.next_token() {
            Some(next) => if next == *token {
                Ok(())
            } else {
                bail!("expected {:?}, found {:?}", token, next);
            },
            None => bail!("expected {:?}, found EOF", token),
        }
    }

    fn parse(mut self) -> Result<Program> {
        let mut program = Vec::new();

        while let Some(token) = self.next_token() {
            if token == Token::Semi {
                continue;
            }

            let stmt = self.parse_stmt(token)?;
            self.assert_token(&Token::Semi)?;
            program.push(stmt);
        }

        Ok(program)
    }

    fn parse_block(&mut self) -> Result<Block> {
        self.assert_token(&Token::LeftBrace)?;

        let mut block = Vec::new();
        loop {
            match self.next_token() {
                Some(Token::RightBrace) => break,
                Some(token) => {
                    let stmt = self.parse_stmt(token)?;
                    self.assert_token(&Token::Semi)?;
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
        let word = assert_word(token)?;
        let stmt = if word == "if" {
            Stmt::If(self.parse_if_stmt()?)
        } else if word == "while" {
            Stmt::While(self.parse_while_stmt()?)
        } else {
            Stmt::Command(self.parse_command(Some(word))?)
        };
        Ok(stmt)
    }

    fn parse_if_stmt(&mut self) -> Result<IfStmt> {
        let test = self.parse_command(None)?;
        let consequent = self.parse_block()?;

        let alternate = if self.match_token(&Token::Word("else".into())) {
            Some(if self.match_token(&Token::Word("if".into())) {
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

    fn parse_command(&mut self, mut name: Option<String>) -> Result<Command> {
        let name = match name.take() {
            Some(name) => name,
            None => assert_word(self.next_token())?,
        };
        let mut command = Command::from_name(name);

        loop {
            match self.next_token().unwrap() {
                Token::Word(argument) => command.add_argument(argument),
                token => {
                    self.push_token(token);
                    break;
                }
            }
        }

        Ok(command)
    }
}

fn assert_word<T>(token: T) -> Result<String>
where
    T: Into<Option<Token>>,
{
    match token.into() {
        Some(token) => if let Token::Word(word) = token {
            Ok(word)
        } else {
            bail!("expected word, found {:?}", token)
        },
        None => bail!("unexpected EOF in word position"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        assert_eq!(
            parse("ls\n").unwrap(),
            vec![Stmt::Command(Command::from_name("ls".into()))],
        );
    }

    #[test]
    fn empty() {
        assert_eq!(parse("\n").unwrap(), Vec::new());
    }

    #[test]
    fn arguments() {
        assert_eq!(
            parse("cat /etc/hosts /etc/passwd\n").unwrap(),
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
            parse("/bin/echo 1  2   3\n").unwrap(),
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
            parse("  cat   \n").unwrap(),
            vec![Stmt::Command(Command::from_name("cat".into()))],
        );
    }

    #[test]
    fn semicolon_and_lf_ends_stmt() {
        assert_eq!(
            parse("echo 1; echo 2\necho 3\n").unwrap(),
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
            parse("if true { echo truthy }\n").unwrap(),
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
