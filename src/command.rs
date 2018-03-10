use std::env;
use std::ffi::CString;

#[derive(PartialEq, Debug)]
pub struct Command {
    name: String,
    arguments: Vec<String>,
}

impl Command {
    pub fn new(name: String, arguments: Vec<String>) -> Self {
        Self { name, arguments }
    }

    pub fn with_name(name: String) -> Self {
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
            Execv::Relative(PathIterator::new(self.name), arguments)
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
    Relative(PathIterator, Vec<CString>),
}

pub struct PathIterator {
    name: String,
    path: Vec<String>,
}

impl PathIterator {
    fn new(name: String) -> Self {
        Self {
            name,
            path: env::var("PATH").expect("PATH required").split(':').rfold(
                Vec::new(),
                |mut path, s| {
                    path.push(s.into());
                    path
                },
            ),
        }
    }
}

impl Iterator for PathIterator {
    type Item = CString;

    fn next(&mut self) -> Option<Self::Item> {
        self.path.pop().map(|mut path| {
            path.push('/');
            path.push_str(&self.name);
            CString::new(path).unwrap()
        })
    }
}
