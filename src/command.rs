use std::ffi::CString;

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    name: String,
    arguments: Vec<String>,
}

impl Command {
    pub fn new(name: String, arguments: Vec<String>) -> Self {
        Self { name, arguments }
    }

    pub fn from_name(name: String) -> Self {
        Self {
            name,
            arguments: Vec::new(),
        }
    }

    pub fn add_argument(&mut self, argument: String) {
        self.arguments.push(argument);
    }

    pub fn into_execv(mut self) -> Execv {
        self.arguments.insert(0, self.name.clone());

        let arguments = self.arguments
            .iter()
            .map(|s| CString::new(s.clone()).unwrap())
            .collect();

        if self.name.contains('/') {
            Execv::Exact(CString::new(self.name).unwrap(), arguments)
        } else {
            Execv::Relative(self.name, arguments)
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
}

pub enum Execv {
    Exact(CString, Vec<CString>),
    Relative(String, Vec<CString>),
}
