use std::borrow::Cow;
use std::ffi::{CString, OsStr};
use std::os::unix::ffi::OsStrExt;

use crate::ast::NameValuePair;
use crate::environment::Environment;
use crate::redirect::Redirect;
use crate::word::Word;
use crate::Result;

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
                Redirect::InFile(ref path) => Redirect::InFile(path.expand(environment)?),
                Redirect::OutErr => Redirect::OutErr,
                Redirect::OutFile(ref path, mode) => {
                    Redirect::OutFile(path.expand(environment)?, mode)
                }
                Redirect::ErrOut => Redirect::ErrOut,
                Redirect::ErrFile(ref path, mode) => {
                    Redirect::ErrFile(path.expand(environment)?, mode)
                }
            });
        }

        let mut env = Vec::new();
        for pair in &self.env {
            env.push((
                Cow::Borrowed(pair.name.value.as_ref()),
                pair.value.expand(environment)?,
            ));
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
pub struct ExpandedCommand<'a> {
    name: Cow<'a, OsStr>,
    arguments: Vec<Cow<'a, OsStr>>,
    redirects: Vec<Redirect<Cow<'a, OsStr>>>,
    env: Vec<(Cow<'a, OsStr>, Cow<'a, OsStr>)>,
    pipeline: Option<Box<ExpandedCommand<'a>>>,
}

impl<'a> ExpandedCommand<'a> {
    pub fn into_execv(mut self, environment: &Environment) -> Execv<'a> {
        self.arguments.insert(0, self.name.clone());

        let arguments = self
            .arguments
            .iter()
            .map(|argument| CString::new(argument.clone().as_bytes()).unwrap())
            .collect();

        let mut env: Vec<_> = environment.iter_exported().map(pair_to_execv).collect();
        env.extend(
            self.env
                .into_iter()
                .map(|(name, value)| pair_to_execv((&name, &value))),
        );

        if self.name.as_bytes().contains(&b'/') {
            Execv::Exact(CString::new(self.name.as_bytes()).unwrap(), arguments, env)
        } else {
            Execv::Relative(self.name, arguments, env)
        }
    }

    pub fn name(&self) -> &OsStr {
        &self.name
    }

    pub fn arguments(&self) -> &[Cow<OsStr>] {
        &self.arguments
    }

    pub fn redirects(&self) -> &[Redirect<Cow<OsStr>>] {
        &self.redirects
    }

    pub fn pipeline(&self) -> Option<&ExpandedCommand> {
        self.pipeline.as_ref().map(AsRef::as_ref)
    }
}

pub enum Execv<'a> {
    Exact(CString, Vec<CString>, Vec<CString>),
    Relative(Cow<'a, OsStr>, Vec<CString>, Vec<CString>),
}

fn pair_to_execv((name, value): (&OsStr, &OsStr)) -> CString {
    let mut buf = Vec::with_capacity(name.len() + value.len() + 2);
    buf.extend_from_slice(name.as_bytes());
    buf.push(b'=');
    buf.extend_from_slice(value.as_bytes());
    CString::new(buf).unwrap()
}
