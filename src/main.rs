#![allow(unreachable_code, unused_variables)]

mod config;
mod dir_diff;
mod pkgcheck;

use std::cmp::Ordering;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::exit;
use std::thread;

use crate::config::Config;
use crate::pkgcheck::Check;

use alpm::Version as alpmVersion;
use aur_client_fork::aur;
use futures::{stream, StreamExt};
use git2::Repository;
use reqwest::Url;

#[tokio::main]
async fn main() {
    let config = match Config::new() {
        Ok((c, b)) => {
            if b {
                println!("Config created");
                exit(0);
            }
            c
        }
        Err(s) => {
            eprintln!("Error reading config: {}", s);
            exit(1);
        }
    };

    if config.need_adjustment() {
        println!("Fill all config options!");
        exit(2);
    }

    if let Err(e) = config.create_environment() {
        eprintln!("Error creating dirs: {}", e);
        exit(1);
    }

    let path = Path::new(&config.repo_dir);
    let rbuild = config.as_rbuild();

    loop {
        refresh_packages(&config, path).await;
        thread::sleep(config.refresh_delay);
    }
}

async fn refresh_packages(config: &config::Config, path: &Path) {
    stream::iter(path.read_dir().unwrap())
        .map(|i| async move { handle_package(&config, i.unwrap(), path).await })
        .buffer_unordered(10)
        .for_each(|b| async {
            if let Err(e) = b {
                println!("{:?}", e);
            }
        })
        .await;
}

/// Checks if a package has updates.
async fn handle_package(
    config: &config::Config,
    i: fs::DirEntry,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    let file_name = i.file_name().to_str().unwrap().to_owned();
    if !file_name.ends_with(".zst") && !file_name.ends_with(".xz") {
        return Ok(());
    }

    println!("found package: {}", file_name);

    let info = pkginfo::new(path.join(&file_name).to_str().unwrap());
    if info.is_err() {
        return Ok(());
    }

    let local_pkg_info = info.unwrap();

    // Filter packages to ignore
    if let Some(ref to_ignore) = config.ignore_packages {
        if to_ignore.contains(&local_pkg_info.pkg_name) {
            return Ok(());
        }
    }

    // Find package in AUR
    let remote_pkg_results = aur::info(&[&local_pkg_info.pkg_name]).await?.results;
    if remote_pkg_results.is_empty() {
        // Package was not found in AUR
        return Ok(());
    }

    let aur_pkg = remote_pkg_results.into_iter().nth(0).unwrap();

    let local_ver = alpmVersion::new(&local_pkg_info.pkg_ver);
    let aur_ver = alpmVersion::new(&aur_pkg.Version);

    // Ignore non updates
    if alpmVersion::cmp(&local_ver, &aur_ver) != Ordering::Less {
        return Ok(());
    }

    println!(
        "Updating {} {} -> {}",
        local_pkg_info.pkg_name, local_pkg_info.pkg_ver, aur_ver,
    );

    update_package(config, aur_pkg, local_pkg_info).await?;
    Ok(())
}

async fn update_package(
    config: &config::Config,
    aur_package: aur::Package,
    local_pkg_info: pkginfo::PkgInfo,
) -> Result<(), Box<dyn Error>> {
    // working dir
    let tmp_path = Path::new(&config.tmp_dir).join(&local_pkg_info.pkg_name);

    let tmp_aur = tmp_path.join("aur"); // Tmp AUR git dir
    let tmp_custom = tmp_path.join("git"); // Tmp custom git dir

    // An existing tmp dir indicates a
    // running package upgrade process
    if tmp_path.exists() {
        println!("Already building for: {}", local_pkg_info.pkg_name);
        return Ok(());
    }

    // Create required files
    fs::create_dir(&tmp_path)?;
    fs::create_dir(&tmp_aur)?;
    fs::create_dir(&tmp_custom)?;

    // Clone custom repo's git version
    let custom_git_url = Url::parse(
        Path::new(&config.git.url)
            .join(&config.git.user)
            .join(&local_pkg_info.pkg_name)
            .to_str()
            .unwrap(),
    )?;
    let custom_repo = Repository::clone(custom_git_url.as_str(), &tmp_custom)?;

    // Clone aur package
    let aur_git_url =
        Url::parse(format!("https://aur.archlinux.org/{}.git", local_pkg_info.pkg_name).as_str())?;
    let aur_repo = Repository::clone(aur_git_url.as_str(), &tmp_aur)?;

    // Create pkg check for local tmp files
    let pkg_check = Check::new(&tmp_custom, &tmp_aur);

    // Check dir-difference
    if pkg_check.are_dirs_different() {
        println!("Dirs are different!");
        return Ok(());
    }

    // check file contents
    if !pkg_check.check_files()? {
        println!("Checks didn't pass for '{}'", local_pkg_info.pkg_name);
        return Ok(());
    }

    // Create remote build job.
    let rbuild = config.as_rbuild();

    let aurbuild = rbuild.new_aurbuild(&local_pkg_info.pkg_name).with_dmanager(
        config.dmanager.user_name.clone(),
        config.dmanager.token.clone(),
        config.dmanager.url.clone(),
        "".to_owned(),
    );

    if let Err(e) = aurbuild.create_job().await {
        eprintln!("Error creating rbuild job: {:?}", e);
        return Ok(());
    }

    Ok(())
}
