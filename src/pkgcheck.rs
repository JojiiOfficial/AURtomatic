#![allow(dead_code)]
use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;

use crate::dir_diff;

/// Check represents the validation of a new AUR package
/// version. It is supposed to reduce the risk of automatically
/// executing the PKGBUILD scripts. This is getting achieved
/// by forbidding unexpected changes.
#[derive(Debug)]
pub struct Check<'a> {
    folder_left: &'a Path,
    folder_right: &'a Path,
}

/// All PKGBUILD changes's prefixes which are allowed
/// to be changed with updates
const ALLOWED_CHANGES: &'static [&'static str] = &[
    "license",
    "pkgver",
    "pkgrel",
    "pkgdesc",
    "arch",
    "sha256sums",
    "sha512sums",
    "md5sums",
    "optdepends",
    "validpgpkeys",
    "conflicts",
    "_pkgver",
    "_pkgrel",
    "_pkgdesc",
];

impl<'a> Check<'a> {
    /// Create a new check
    pub fn new(folder_left: &'a Path, folder_right: &'a Path) -> Self {
        Check {
            folder_left,  // folder_left is the local git version
            folder_right, // folder_right is the remote version
        }
    }

    /// Check if there are new files in the AUR version
    pub fn are_dirs_different(&self) -> bool {
        if let Some(site) = dir_diff::is_different(self.folder_left, self.folder_right).unwrap() {
            return site == dir_diff::Site::Right;
        }
        false
    }

    /// Check all files by comparing the differences of the git version and the
    /// new AUR package version.
    pub fn check_files(&self) -> Result<bool, Box<dyn Error>> {
        // Zip up all git files and the corresponding updated files
        for (a, b) in dir_diff::walk_dir(self.folder_left)?
            .filter_entry(dir_diff::git_filter_entries)
            .zip(dir_diff::walk_dir(self.folder_right)?.filter_entry(dir_diff::git_filter_entries))
        {
            let a = a?; // local file
            let b = b?; // remote file

            if a.file_type().is_dir() || b.file_type().is_dir() {
                continue;
            }

            let a_content = read_file(a.path())?;
            let b_content = read_file(b.path())?;

            //  Build diff from both file contents
            let diff_result = diff::lines(a_content.as_str(), b_content.as_str());

            // Check and validate the upgraded package
            if !Self::check_diff_result(&diff_result) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Returns false if the AUR file contains illegal changes
    fn check_diff_result(result: &Vec<diff::Result<&str>>) -> bool {
        // Go through every created diff
        for diff in result {
            if let diff::Result::Right(r) = diff {
                // All non-variable changes are forbidden
                if !r.contains("=") {
                    eprintln!("Changed '{}' Which has no '=' -> Illegal change", r);
                    return false;
                }

                let s = r.split("=").nth(0).unwrap();
                // Check if the variable update is allowed
                if !ALLOWED_CHANGES.contains(&s) {
                    eprintln!("Found '{}' -> Illegal change", s);
                    return false;
                }
            }
        }

        true
    }
}

/// Read file and remove empty lines
fn read_file(p: &Path) -> Result<String, io::Error> {
    let mut s = String::new();

    for i in fs::read_to_string(p)?.lines() {
        // Ignore empty lines and comments
        if i.trim().is_empty() || i.trim().starts_with("#") {
            continue;
        }

        s.push_str(i.replace(";", ";\n").as_str());
        s.push('\n');
    }

    Ok(s)
}

/// Handy function to debug the changes.
fn debug_diff_result<'a>(res: &Vec<diff::Result<&'a str>>) {
    for diff in res {
        match diff {
            diff::Result::Left(l) => println!("-{}", l),
            diff::Result::Both(l, _) => println!(" {}", l),
            diff::Result::Right(r) => println!("+{}", r),
        }
    }
}
