#![allow(unreachable_code, unused_variables)]

mod config;
mod dir_diff;
mod error;
mod pkgcheck;
mod tg_bot_wrapper;

use std::cmp::Ordering;
use std::error::Error as stdErr;
use std::fs;
use std::path::Path;
use std::process::exit;
use std::thread;
use std::time::Duration;

use crate::config::Config;
use crate::error::Error;
use crate::pkgcheck::Check;

use alpm::Version as alpmVersion;
use async_std::task;
use aur_client_fork::aur;
use futures::{stream, StreamExt};
use git2::Repository;
use lib_remotebuild_rs::jobs::Status as jobStatus;
use lib_remotebuild_rs::librb::LibRb;
use reqwest::Url;
use tg_bot_wrapper::TgBot;

struct BuildService {
    config: Config,
    tgbot: TgBot,
}

impl BuildService {
    fn new(config: config::Config, tgbot: TgBot) -> Self {
        BuildService { config, tgbot }
    }

    async fn run(&self) {
        self.tgbot
            .send_message(self.config.telegram.user_id, "Bot started")
            .await
            .unwrap();

        loop {
            self.refresh_packages(&self.config).await;
            thread::sleep(self.config.refresh_delay);
        }
    }

    async fn refresh_packages(&self, config: &config::Config) {
        let path = Path::new(&config.repo_dir);

        stream::iter(path.read_dir().unwrap())
            .map(|i| async move { self.handle_package(&config, i.unwrap(), path).await })
            .buffer_unordered(10)
            .for_each(|b| async {
                if let Err(e) = b {
                    self.tgbot
                        .send_message(self.config.telegram.user_id, format!("{:?}", e))
                        .await
                        .unwrap();
                    println!("{:?}", e);
                }
            })
            .await;
    }

    /// Checks if a package has updates.
    async fn handle_package(
        &self,
        config: &config::Config,
        i: fs::DirEntry,
        path: &Path,
    ) -> Result<(), Box<dyn stdErr>> {
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

        self.update_package(config, aur_pkg, local_pkg_info).await?;
        Ok(())
    }

    async fn update_package(
        &self,
        config: &config::Config,
        aur_package: aur::Package,
        local_pkg_info: pkginfo::PkgInfo,
    ) -> Result<(), Box<dyn stdErr>> {
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
                .join(&local_pkg_info.pkg_name)
                .to_str()
                .unwrap(),
        )?;

        // Clone aur package
        let aur_git_url = Url::parse(
            format!("https://aur.archlinux.org/{}.git", local_pkg_info.pkg_name).as_str(),
        )?;
        let aur_repo = Repository::clone(aur_git_url.as_str(), &tmp_aur)?;

        let mut cb = git2::RemoteCallbacks::new();
        cb.credentials(|a, b, c| self.get_ssh_auth(a, b, c));

        let mut fo = git2::FetchOptions::new();
        fo.remote_callbacks(cb);
        let custom_repo = git2::build::RepoBuilder::new()
            .fetch_options(fo)
            .clone(custom_git_url.as_str(), &tmp_custom)?;

        // Create pkg check for local tmp files
        let pkg_check = Check::new(&tmp_custom, &tmp_aur);

        // Check dir-difference
        if pkg_check.are_dirs_different() {
            return Err(Box::new(Error::DifferentDirs(local_pkg_info.pkg_name)));
        }

        // check file contents
        if !pkg_check.check_files()? {
            //return Err(Box::new(Error::ChecksFailed(local_pkg_info.pkg_name)));
            return Ok(());
        }

        pkg_check.apply_changes()?;
        pkg_check.update_custom_srcinfo().await?;

        // Create remote build job.
        let rbuild = config.as_rbuild();

        let aurbuild = rbuild.new_aurbuild(&local_pkg_info.pkg_name).with_dmanager(
            config.dmanager.user_name.clone(),
            config.dmanager.token.clone(),
            config.dmanager.url.clone(),
            "".to_owned(),
        );

        // Create BuildJob
        let build_job = aurbuild.create_job().await;
        if let Err(e) = build_job {
            return Err(Box::new(Error::AurJobError(local_pkg_info.pkg_name)));
        }

        let build_job = build_job.unwrap();
        let job_id = build_job.response.unwrap().id;
        println!("Created Job with ID: {}", job_id);

        // Wait here until job is done
        if let Err(e) = self.wait_for_build_job(&rbuild, &job_id).await {
            return Err(Box::new(e));
        }

        // Push aur changes to custom git server
        self.apply_custom_repo_changes(&custom_repo, &aur_package)?;

        // Notify user
        self.tgbot
            .send_message(
                config.telegram.user_id,
                format!(
                    "Bulit package {} version {}",
                    aur_package.Name, aur_package.Version
                ),
            )
            .await?;

        // Download built package

        // Sign package

        // Publish package

        // Delete tmp folder
        fs::remove_dir_all(tmp_path)?;

        Ok(())
    }

    fn get_ssh_auth(
        &self,
        a: &str,
        b: Option<&str>,
        c: git2::CredentialType,
    ) -> Result<git2::Cred, git2::Error> {
        let key =
            fs::read_to_string(Path::new(config::CONFIG_PATH).join(&self.config.git.priv_key))
                .expect("Can't read priv_key");

        Ok(git2::Cred::ssh_key_from_memory(
            b.unwrap(),
            None,
            &key,
            None,
        )?)
    }

    /// Commit changes froum AUR and push them back
    /// to the server
    fn apply_custom_repo_changes(
        &self,
        custom_repo: &git2::Repository,
        aur_package: &aur_client_fork::aur::Package,
    ) -> Result<(), Box<dyn stdErr>> {
        let mut custom_repo_index = custom_repo.index()?;

        // Add all to git index
        custom_repo_index.add_all(["."].iter(), git2::IndexAddOption::DEFAULT, None)?;
        custom_repo_index.write()?;

        // Create commit
        let sig = git2::Signature::now(&self.config.git.bot_name, &self.config.git.bot_email)?;
        let commit = custom_repo.find_commit(custom_repo.head()?.target().unwrap())?;
        let tree = custom_repo.find_tree(custom_repo_index.write_tree()?)?;

        let nice_aur_version = {
            if !aur_package.Version.starts_with("v") {
                format!("v{}", aur_package.Version)
            } else {
                aur_package.Version.clone()
            }
        };

        custom_repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            format!("Update to AUR {}", nice_aur_version).as_str(),
            &tree,
            &[&commit],
        )?;

        // Push changes
        let mut cb = git2::RemoteCallbacks::new();
        cb.credentials(|a, b, c| self.get_ssh_auth(a, b, c));

        let mut push_option = git2::PushOptions::new();
        push_option.remote_callbacks(cb);

        custom_repo.find_remote("origin")?.push(
            &["refs/heads/master:refs/heads/master"],
            Some(&mut push_option),
        )?;
        println!("push done");

        Ok(())
    }

    async fn wait_for_build_job(&self, rbuild: &LibRb, jid: &u32) -> Result<(), Error> {
        let info = loop {
            let info = rbuild.job_info(*jid).await;

            if let Err(e) = info {
                return Err(Error::JobInfoError(format!("{:?}", e)));
            }

            let info = info.unwrap().response.unwrap();
            if info.status.is_stopped_state() {
                break info;
            }

            task::sleep(Duration::from_secs(60)).await;
        };

        match info.status {
            jobStatus::Failed => Err(Error::JobFailed(format!("{}", jid))),
            jobStatus::Cancelled => {
                Err(Error::JobFailed(format!("ID: {}. Job was cancelled", jid)))
            }
            _ => Ok(()),
        }
    }
}

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

    let tg_bot = tg_bot_wrapper::TgBot::new(config.telegram.bot_token.clone());
    let build_service = BuildService::new(config, tg_bot);

    build_service.run().await;
}
