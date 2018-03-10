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
        while self.has_remaining_tokens() {
            let statement = self.parse_statement()?;
            self.program.push(statement);
        }

        Ok(self.program)
    }

    fn has_remaining_tokens(&mut self) -> bool {
        self.lexer.peek().is_some()
    }

    fn parse_statement(&mut self) -> Result<Statement> {
        let token = self.lexer.next().expect("no tokens left");

        match token {
            Token::Word(word) => Ok(Statement::Command(self.finish_command(word)?)),
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
}
