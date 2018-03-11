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
    condition: Command,
    then_clause: Block,
    else_clause: Option<Block>,
}

impl IfStmt {
    pub fn new(condition: Command, then_clause: Block, else_clause: Option<Block>) -> Self {
        Self {
            condition,
            then_clause,
            else_clause,
        }
    }
}
