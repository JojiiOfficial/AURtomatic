use std::error::Error as stdErr;
use std::fmt::{Display, Formatter, Result};

#[derive(Debug)]
pub enum Error {
    DifferentDirs(String),
    ChecksFailed(String),
    AurJobError(String),
    JobInfoError(String),
    JobFailed(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{:?}", self)
    }
}

impl stdErr for Error {}
