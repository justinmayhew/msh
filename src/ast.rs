use crate::command::Command;
use crate::word::Word;

pub type Program = Block;
pub type Block = Vec<Stmt>;

#[derive(Debug, PartialEq)]
pub enum Stmt {
    If(IfStmt),
    While(WhileStmt),
    Export(Vec<Exportable>),
    Assignment(Vec<NameValuePair>),
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

#[derive(Debug, PartialEq)]
pub struct Exportable {
    pub name: Word,
    pub value: Option<Word>,
}

impl Exportable {
    pub fn new(name: Word, value: Option<Word>) -> Self {
        Self { name, value }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NameValuePair {
    pub name: Word,
    pub value: Word,
}

impl NameValuePair {
    pub fn new(name: Word, value: Word) -> Self {
        Self { name, value }
    }
}
