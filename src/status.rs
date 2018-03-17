use libc;

/// The exit status of a command.
pub enum Status {
    Success,
    Failure,
}

impl Status {
    pub fn is_success(&self) -> bool {
        match *self {
            Status::Success => true,
            Status::Failure => false,
        }
    }
}

impl From<i32> for Status {
    fn from(code: i32) -> Status {
        if code == libc::EXIT_SUCCESS {
            Status::Success
        } else {
            Status::Failure
        }
    }
}
