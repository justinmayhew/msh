use command::Command;

pub type Program = Block;
pub type Block = Vec<Stmt>;

#[derive(Debug, PartialEq)]
pub enum Stmt {
    If(IfStmt),
    Command(Command),
}

#[derive(Debug, PartialEq)]
pub struct IfStmt {
    pub test: Command,
    pub consequent: Block,
    pub alternate: Option<Block>,
}

impl IfStmt {
    pub fn new(test: Command, consequent: Block, alternate: Option<Block>) -> Self {
        Self {
            test,
            consequent,
            alternate,
        }
    }
}
