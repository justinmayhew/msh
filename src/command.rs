use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    name: OsString,
    arguments: Vec<OsString>,
}

impl Command {
    pub fn new(name: OsString, arguments: Vec<OsString>) -> Self {
        Self { name, arguments }
    }

    pub fn from_name(name: OsString) -> Self {
        Self {
            name,
            arguments: Vec::new(),
        }
    }

    pub fn add_argument(&mut self, argument: OsString) {
        self.arguments.push(argument);
    }

    pub fn into_execv(mut self) -> Execv {
        self.arguments.insert(0, self.name.clone());

        let arguments = self.arguments
            .iter()
            .map(|argument| CString::new(argument.clone().into_vec()).unwrap())
            .collect();

        if self.name.as_bytes().contains(&b'/') {
            Execv::Exact(CString::new(self.name.into_vec()).unwrap(), arguments)
        } else {
            Execv::Relative(self.name, arguments)
        }
    }

    pub fn name(&self) -> &OsStr {
        &self.name
    }

    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }
}

pub enum Execv {
    Exact(CString, Vec<CString>),
    Relative(OsString, Vec<CString>),
}
