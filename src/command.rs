use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;

use Result;
use ast::NameValuePair;
use environment::Environment;
use redirect::Redirect;
use word::Word;

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    name: Word,
    arguments: Vec<Word>,
    redirects: Vec<Redirect<Word>>,
    env: Vec<NameValuePair>,
    pipeline: Option<Box<Command>>,
}

impl Command {
    pub fn new(name: Word, arguments: Vec<Word>) -> Self {
        Self {
            name,
            arguments,
            redirects: Vec::new(),
            env: Vec::new(),
            pipeline: None,
        }
    }

    pub fn from_name(name: Word) -> Self {
        Self::new(name, Vec::new())
    }

    pub fn add_argument(&mut self, argument: Word) {
        self.arguments.push(argument);
    }

    pub fn add_redirect(&mut self, redirect: Redirect<Word>) {
        self.redirects.push(redirect);
    }

    pub fn set_env(&mut self, env: Vec<NameValuePair>) {
        self.env = env;
    }

    pub fn set_pipeline(&mut self, pipeline: Command) {
        self.pipeline = Some(Box::new(pipeline));
    }

    pub fn expand(&self, environment: &Environment) -> Result<ExpandedCommand> {
        let name = self.name.expand(environment)?;

        let mut arguments = Vec::new();
        for argument in &self.arguments {
            arguments.push(argument.expand(environment)?);
        }

        let mut redirects = Vec::new();
        for redirect in &self.redirects {
            redirects.push(match *redirect {
                Redirect::InFile(ref path) => {
                    Redirect::InFile(PathBuf::from(path.expand(environment)?))
                }
                Redirect::OutErr => Redirect::OutErr,
                Redirect::OutFile(ref path, mode) => {
                    Redirect::OutFile(PathBuf::from(path.expand(environment)?), mode)
                }
                Redirect::ErrOut => Redirect::ErrOut,
                Redirect::ErrFile(ref path, mode) => {
                    Redirect::ErrFile(PathBuf::from(path.expand(environment)?), mode)
                }
            });
        }

        let mut env = Vec::new();
        for pair in &self.env {
            env.push((pair.name.to_os_string(), pair.value.expand(environment)?));
        }

        Ok(ExpandedCommand {
            name,
            arguments,
            redirects,
            env,
            pipeline: match self.pipeline {
                Some(ref cmd) => Some(Box::new(cmd.expand(environment)?)),
                None => None,
            },
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExpandedCommand {
    name: OsString,
    arguments: Vec<OsString>,
    redirects: Vec<Redirect<PathBuf>>,
    env: Vec<(OsString, OsString)>,
    pipeline: Option<Box<ExpandedCommand>>,
}

impl ExpandedCommand {
    pub fn into_execv(mut self, environment: &Environment) -> Execv {
        self.arguments.insert(0, self.name.clone());

        let arguments = self.arguments
            .iter()
            .map(|argument| CString::new(argument.clone().into_vec()).unwrap())
            .collect();

        let mut env: Vec<_> = environment
            .iter_exported()
            .map(|(name, value)| pair_to_execv((name.to_owned(), value.to_owned())))
            .collect();
        env.extend(self.env.into_iter().map(pair_to_execv));

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

    pub fn redirects(&self) -> &[Redirect<PathBuf>] {
        &self.redirects
    }

    pub fn pipeline(&self) -> Option<&ExpandedCommand> {
        self.pipeline.as_ref().map(AsRef::as_ref)
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
