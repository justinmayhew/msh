use std::env;
use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

use Result;
use ast::Assignment;
use word::Word;

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    name: Word,
    arguments: Vec<Word>,
    env: Assignment,
}

impl Command {
    #[cfg(test)]
    pub fn new(name: Word, arguments: Vec<Word>) -> Self {
        Self {
            name,
            arguments,
            env: Vec::new(),
        }
    }

    pub fn from_name(name: Word) -> Self {
        Self {
            name,
            arguments: Vec::new(),
            env: Vec::new(),
        }
    }

    pub fn add_argument(&mut self, argument: Word) {
        self.arguments.push(argument);
    }

    pub fn set_env(&mut self, env: Assignment) {
        self.env = env;
    }

    pub fn expand<P: AsRef<Path>>(&self, home: P) -> Result<ExpandedCommand> {
        let name = self.name.expand(home.as_ref())?;

        let mut arguments = Vec::new();
        for argument in &self.arguments {
            arguments.push(argument.expand(home.as_ref())?);
        }

        let mut env = Vec::new();
        for &(ref name, ref value) in &self.env {
            env.push((name.to_os_string(), value.expand(home.as_ref())?));
        }

        Ok(ExpandedCommand {
            name,
            arguments,
            env,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExpandedCommand {
    name: OsString,
    arguments: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
}

impl ExpandedCommand {
    pub fn into_execv(mut self) -> Execv {
        self.arguments.insert(0, self.name.clone());

        let arguments = self.arguments
            .iter()
            .map(|argument| CString::new(argument.clone().into_vec()).unwrap())
            .collect();

        let mut env: Vec<_> = env::vars_os().map(pair_to_execv).collect();
        env.extend(self.env.iter().cloned().map(pair_to_execv));

        if self.name.as_bytes().contains(&b'/') {
            Execv::Exact(CString::new(self.name.into_vec()).unwrap(), arguments, env)
        } else {
            Execv::Relative(self.name, arguments, env)
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
    Exact(CString, Vec<CString>, Vec<CString>),
    Relative(OsString, Vec<CString>, Vec<CString>),
}

fn pair_to_execv((mut name, value): (OsString, OsString)) -> CString {
    name.push("=");
    name.push(value);
    CString::new(name.as_bytes()).unwrap()
}
