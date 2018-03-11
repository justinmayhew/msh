use command::Command;

pub type Program = Block;
pub type Block = Vec<Stmt>;

#[derive(Debug, PartialEq)]
pub enum Stmt {
    If(IfStmt),
    While(WhileStmt),
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

#[derive(Debug, PartialEq)]
pub struct WhileStmt {
    pub test: Command,
    pub body: Block,
}

impl WhileStmt {
    pub fn new(test: Command, body: Block) -> Self {
        Self { test, body }
    }
}
