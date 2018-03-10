use command::Command;

pub type Program = Vec<Statement>;

#[derive(PartialEq, Debug)]
pub enum Statement {
    Command(Command),
}
