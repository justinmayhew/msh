use std::iter::Peekable;

use Result;
use ast::{Program, Statement};
use command::Command;
use lexer::{Lexer, Token};

struct Parser<'input> {
    lexer: Peekable<Lexer<'input>>,
    program: Program,
}

impl<'input> Parser<'input> {
    fn new(src: &'input str) -> Self {
        Self {
            lexer: Lexer::new(src).peekable(),
            program: Vec::new(),
        }
    }

    fn parse(mut self) -> Result<Program> {
        while let Some(token) = self.lexer.next() {
            if let Some(statement) = self.parse_statement(token)? {
                self.program.push(statement);
            }
        }

        Ok(self.program)
    }

    fn parse_statement(&mut self, token: Token) -> Result<Option<Statement>> {
        match token {
            Token::Word(word) => Ok(Some(Statement::Command(self.finish_command(word)?))),
            Token::Delimiter => Ok(None),
        }
    }

    fn finish_command(&mut self, name: String) -> Result<Command> {
        let mut command = Command::from_name(name);

        while let Some(Token::Word(argument)) = self.lexer.next() {
            command.add_argument(argument);
        }

        Ok(command)
    }
}

pub fn parse(input: &str) -> Result<Program> {
    Parser::new(input).parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        let cmd = Command::from_name("ls".into());
        assert_eq!(parse("ls\n").unwrap(), vec![Statement::Command(cmd)]);
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
                Statement::Command(Command::new(
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
                Statement::Command(Command::new(
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
            vec![Statement::Command(Command::from_name("cat".into()))],
        );
    }

    #[test]
    fn delimiters() {
        assert_eq!(
            parse("echo 1; echo 2\necho 3").unwrap(),
            vec![
                Statement::Command(Command::new("echo".into(), vec!["1".into()])),
                Statement::Command(Command::new("echo".into(), vec!["2".into()])),
                Statement::Command(Command::new("echo".into(), vec!["3".into()])),
            ],
        );
    }
}
