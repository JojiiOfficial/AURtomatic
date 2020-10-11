#![allow(dead_code)]

use std::error::Error;
use std::fs::{self, File};
use std::io::{self, prelude::*};
use std::path::Path;

use md5;
use regex::Regex;
use tokio::process::Command;
use tree_magic;

use crate::dir_diff;

#[cfg(test)]
#[path = "pkgcheck_test.rs"]
mod pkgcheck_test;

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
    "sha256sums_armv7h",
    "sha256sums_aarch64",
    "sha256sums_x86_64",
    "depends",
    "_pkgname",
];

/// All MIMES which are allowed to be changed in updates.
const ALLOWED_MIMES: &'static [&'static str] = &["image/"];

/// All MIMES which will be go through diff checks
const UTF8_MIMES: &'static [&'static str] = &[
    "text/",
    "application/x-shellscript",
    "application/x-desktop",
    "application/mbox",
    "application/xml",
    "application/json",
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
    pub fn check_files(&self, check_diff: bool) -> Result<bool, Box<dyn Error>> {
        let mut had_diff = false;

        // Zip up all git files and the corresponding updated files
        for (a, b) in dir_diff::walk_dir(self.folder_left)?
            .filter_entry(dir_diff::git_filter_entries)
            .zip(dir_diff::walk_dir(self.folder_right)?.filter_entry(dir_diff::git_filter_entries))
        {
            let a = a?; // local file
            let b = b?; // remote file

            if a.file_type().is_dir() || b.file_type().is_dir() {
                continue;
            };

            let mime = get_mime(b.path())?;
            if partial_contains(UTF8_MIMES, mime) {
                println!("utf8-mime: {}", mime);
                let a_content = parse_src_file(fs::read_to_string(a.path())?);
                let b_content = parse_src_file(fs::read_to_string(b.path())?);

                //  Build diff from both file contents
                let diff = diff::lines(a_content.as_str(), b_content.as_str());
                if !is_diff_empty(&diff) {
                    had_diff = true;
                }

                // Check and validate the upgraded package
                if check_diff && !Self::check_diff(diff, a.file_name().to_str().unwrap()) {
                    return Ok(false);
                }
            } else {
                println!("Non utf8-mime: {}", mime);
                let has_diff = hash_file_diff(&a.path(), &b.path())?;

                if check_diff && !partial_contains(ALLOWED_MIMES, mime) && has_diff {
                    // Throw error if mime doesn't allow changing
                    println!("Hashsum check failed: {}", b.path().display());
                    return Ok(false);
                }

                if has_diff {
                    had_diff = true;
                }
            }
        }

        if !had_diff {
            println!("No change detected!");
            return Ok(false);
        }

        Ok(true)
    }

    /// Returns false if the AUR file contains illegal changes
    fn check_diff(res: Vec<diff::Result<&str>>, file: &str) -> bool {
        // Go through every created diff
        for diff in res {
            if let diff::Result::Right(r) = diff {
                // All non-variable changes are forbidden
                if !r.contains("=") {
                    eprintln!("Changed '{}' Which has no '=' -> Illegal change", r);
                    return false;
                }

                let s = r.split("=").nth(0).unwrap();
                // Check if the variable update is allowed. Custom variables are allowed
                if !ALLOWED_CHANGES.contains(&s) && !s.starts_with("_") {
                    eprintln!("Found '{}' -> Illegal change in {}", s, file);
                    return false;
                }
            }
        }

        true
    }

    /// Apply changes from aur to own repo
    pub fn apply_changes(&self) -> Result<(), io::Error> {
        for (a, b) in dir_diff::walk_dir(self.folder_left)?
            .filter_entry(dir_diff::git_filter_entries)
            .zip(dir_diff::walk_dir(self.folder_right)?.filter_entry(dir_diff::git_filter_entries))
        {
            let a = a?; // local file
            let b = b?; // remote file

            // Copy filecontents to own git
            fs::copy(b.path(), a.path())?;
        }

        Ok(())
    }

    pub async fn update_custom_srcinfo(&self) -> Result<(), Box<dyn Error>> {
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "pushd \"{}\" > /dev/null; makepkg --printsrcinfo > .SRCINFO",
                self.folder_left.to_str().unwrap()
            ))
            .status()
            .await?;

        Ok(())
    }
}

/// Read file and remove empty lines
fn parse_src_file(src: String) -> String {
    let mut s = String::new();

    let src = unwrap_multi_line(&src, "'\n");

    for i in src.lines() {
        // Ignore empty lines and comments
        if i.trim().is_empty() || i.trim().starts_with("#") {
            continue;
        }

        let m = i.trim().replace(";", ";\n");
        s.push_str(m.as_str());
        s.push('\n');
    }

    s
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

fn is_diff_empty(d: &Vec<diff::Result<&str>>) -> bool {
    for i in d {
        if let diff::Result::Right(_) = i {
            return false;
        }
    }

    true
}

fn unwrap_multi_line(a: &str, sub: &str) -> String {
    Regex::new("[ ]+")
        .unwrap()
        .replace_all(String::from(a).trim().replace(sub, "' ").as_str(), " ")
        .to_string()
}

fn partial_contains<'b, R>(v: R, has: &str) -> bool
where
    R: IntoIterator<Item = &'b &'b str>,
{
    for i in v.into_iter() {
        if *i == has || has.starts_with(i) {
            return true;
        }
    }
    false
}

fn get_mime<'b>(path: &'b Path) -> Result<&'b str, io::Error> {
    let mut buffer = Vec::new();
    get_file_contents(&mut buffer, path)?;
    Ok(tree_magic::from_u8(&buffer))
}

fn hash_file_diff(a: &Path, b: &Path) -> Result<bool, io::Error> {
    Ok(get_file_md5(a)? == get_file_md5(b)?)
}

fn get_file_md5(path: &Path) -> Result<String, io::Error> {
    let mut buffer: Vec<u8> = Vec::new();
    get_file_contents(&mut buffer, path)?;
    Ok(format!("{:x}", md5::compute(buffer)))
}

fn get_file_contents(buffer: &mut Vec<u8>, path: &Path) -> Result<(), io::Error> {
    let mut f = File::open(path)?;
    f.read_to_end(buffer)?;
    Ok(())
}
