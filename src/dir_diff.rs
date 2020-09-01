extern crate walkdir;

use std::cmp::Ordering;
use std::path::Path;

use walkdir::{DirEntry, WalkDir};

/// The various errors that can happen when diffing two directories
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    StripPrefix(std::path::StripPrefixError),
    WalkDir(walkdir::Error),
}

#[derive(Debug, PartialEq)]
pub enum Site {
    Left,
    Right,
    Unknown,
}

pub fn git_filter_entries(f: &DirEntry) -> bool {
    !((f.file_type().is_dir() && f.file_name() == ".git")
        || f.file_name() == ".gitignore"
        || f.file_name() == ".SRCINFO")
}

/// Check if directories are different. On difference detected,
/// return the site which caused the difference. This
/// only applies to additional files.
pub fn is_different<A: AsRef<Path>, B: AsRef<Path>>(
    a_base: A,
    b_base: B,
) -> Result<Option<Site>, Error> {
    let mut a_walker = walk_dir(a_base)?.filter_entry(git_filter_entries);
    let mut b_walker = walk_dir(b_base)?.filter_entry(git_filter_entries);

    for (a, b) in (&mut a_walker).zip(&mut b_walker) {
        let a = a?;
        let b = b?;

        if a.depth() != b.depth()
            || a.file_type() != b.file_type()
            || a.file_name() != b.file_name()
        {
            return Ok(Some(Site::Unknown));
        }
    }

    if a_walker.next().is_some() {
        return Ok(Some(Site::Left));
    }
    if b_walker.next().is_some() {
        return Ok(Some(Site::Right));
    }
    Ok(None)
}

pub fn walk_dir<P: AsRef<Path>>(path: P) -> Result<walkdir::IntoIter, std::io::Error> {
    let mut walkdir = WalkDir::new(path).sort_by(compare_by_file_name).into_iter();
    if let Some(Err(e)) = walkdir.next() {
        Err(e.into())
    } else {
        Ok(walkdir)
    }
}

pub fn compare_by_file_name(a: &DirEntry, b: &DirEntry) -> Ordering {
    a.file_name().cmp(b.file_name())
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

impl From<std::path::StripPrefixError> for Error {
    fn from(e: std::path::StripPrefixError) -> Error {
        Error::StripPrefix(e)
    }
}

impl From<walkdir::Error> for Error {
    fn from(e: walkdir::Error) -> Error {
        Error::WalkDir(e)
    }
}
