#![allow(unreachable_code, unused_variables)]

mod config;

use std::cmp::Ordering;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::exit;

use crate::config::Config;

use alpm::Version as alpmVersion;
use aur_client_fork::aur;
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

    // let tmp_path = Path::new(&config.tmp_dir);
    let path = Path::new(&config.repo_dir);
    let rbuild = config.as_rbuild();

    // Read every file in path
    for i in path.read_dir().unwrap() {
        let file_name = i.unwrap().file_name().to_str().unwrap().to_owned();
        if !file_name.ends_with(".zst") && !file_name.ends_with(".xz") {
            continue;
        }

        let info = pkginfo::new(path.join(&file_name).to_str().unwrap());
        if info.is_err() {
            continue;
        }

        if let Err(e) = handle_package(&config, info.unwrap(), file_name, path).await {
            eprintln!("{}", e);
        }
    }
}

async fn handle_package(
    config: &config::Config,
    local_pkg_info: pkginfo::PkgInfo,
    file_name: String,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    // Find package in AUR
    let remote_pkg_results = { aur::info(&[&local_pkg_info.pkg_name]).await?.results };
    if remote_pkg_results.is_empty() {
        return Ok(());
    }

    let aur_pkg = remote_pkg_results.into_iter().nth(0).unwrap();

    let local_ver = alpmVersion::new(&local_pkg_info.pkg_ver);
    let rem_ver = alpmVersion::new(&aur_pkg.Version);

    // Ignore non updates
    if alpmVersion::cmp(&local_ver, &rem_ver) != Ordering::Less {
        return Ok(());
    }

    println!(
        "Updating {} {} -> {}",
        local_pkg_info.pkg_name, local_pkg_info.pkg_ver, rem_ver,
    );

    update_package(config, aur_pkg, local_pkg_info).await?;
    Ok(())
}

async fn update_package(
    config: &config::Config,
    aur_package: aur::Package,
    local_pkg_info: pkginfo::PkgInfo,
) -> Result<(), Box<dyn Error>> {
    let tmp_path = Path::new(&config.tmp_dir).join(&local_pkg_info.pkg_name);
    let tmp_aur = tmp_path.join("aur");
    let tmp_custom = tmp_path.join("gitea");

    if tmp_path.exists() {
        println!("Already building for: {}", local_pkg_info.pkg_name);
        return Ok(());
    }
    fs::create_dir(&tmp_path)?;
    fs::create_dir(&tmp_aur)?;
    fs::create_dir(&tmp_custom)?;

    let custom_git_url = Url::parse(
        Path::new(&config.git.url)
            .join(&config.git.user)
            .join(&local_pkg_info.pkg_name)
            .to_str()
            .unwrap(),
    )?;
    let custom_repo = Repository::clone(custom_git_url.as_str(), &tmp_custom)?;

    let aur_git_url =
        Url::parse(format!("https://aur.archlinux.org/{}.git", local_pkg_info.pkg_name).as_str())?;
    let aur_repo = Repository::clone(aur_git_url.as_str(), &tmp_aur)?;

    Ok(())
}
