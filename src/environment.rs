use std::collections::HashMap;
use std::collections::hash_map::{Entry, Iter};
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::Path;

use Result;
use ast::{Exportable, NameValuePair};

pub struct Environment {
    values: HashMap<OsString, Var>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            values: env::vars_os()
                .map(|(name, value)| (name, Var::new(value, true)))
                .collect(),
        }
    }

    pub fn get<N: AsRef<OsStr>>(&self, name: N) -> Option<&OsStr> {
        self.values.get(name.as_ref()).map(|var| var.value.as_ref())
    }

    pub fn assign(&mut self, pair: &NameValuePair) -> Result<()> {
        let value = pair.value.expand(self)?;
        match self.values.entry(pair.name.to_os_string()) {
            Entry::Occupied(mut entry) => entry.get_mut().value = value,
            Entry::Vacant(entry) => {
                entry.insert(Var::new(value, false));
            }
        }
        Ok(())
    }

    pub fn export(&mut self, exportable: &Exportable) -> Result<()> {
        if let Some(ref value) = exportable.value {
            let var = Var::new(value.expand(self)?, true);
            self.values.insert(exportable.name.to_os_string(), var);
        } else {
            match self.values.entry(exportable.name.to_os_string()) {
                Entry::Occupied(mut entry) => entry.get_mut().is_exported = true,
                Entry::Vacant(entry) => {
                    entry.insert(Var::new(OsString::from(""), true));
                }
            }
        }
        Ok(())
    }

    pub fn home(&self) -> &Path {
        Path::new(self.get("HOME").expect("HOME required"))
    }

    pub fn path(&self) -> &OsStr {
        match self.get("PATH") {
            Some(value) => value,
            None => OsStr::new(""),
        }
    }

    pub fn iter_exported(&self) -> IterExported {
        IterExported {
            iter: self.values.iter(),
        }
    }
}

struct Var {
    value: OsString,
    is_exported: bool,
}

impl Var {
    fn new(value: OsString, is_exported: bool) -> Self {
        Self { value, is_exported }
    }
}

pub struct IterExported<'env> {
    iter: Iter<'env, OsString, Var>,
}

impl<'env> Iterator for IterExported<'env> {
    type Item = (&'env OsStr, &'env OsStr);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((name, var)) = self.iter.next() {
            if var.is_exported {
                return Some((name, &var.value));
            }
        }
        None
    }
}
