use lib_remotebuild_rs::{config::RequestConfig, librb};
use serde::{Deserialize, Serialize};
use serde_yaml::from_str;

use std::error;
use std::fs;
use std::fs::{create_dir_all, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Duration;

/// The defalut config path.
pub const CONFIG_PATH: &str = "./data/";
pub const CONFIG_FILE: &str = "config.yaml";

/// Whole config struct
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Config {
    pub repo_dir: String,
    pub tmp_dir: String,
    pub rbuild: TokenConfig,
    pub dmanager: TokenConfig,
    pub git: Git,
    pub ignore_packages: Option<Vec<String>>,
    pub refresh_delay: Duration,
}

/// Git upstream for custom repository.
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Git {
    pub bot_name: String,
    pub bot_email: String,

    pub url: String,
    pub priv_key: String,
}

/// RemoteBuild configuration.
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct TokenConfig {
    pub user_name: String,
    pub token: String,
    pub url: String,
}

impl TokenConfig {
    fn is_empty(&self) -> bool {
        self.user_name.is_empty() || self.token.is_empty() || self.url.is_empty()
    }
}

impl Git {
    fn is_empty(&self) -> bool {
        self.url.is_empty()
            || self.bot_email.is_empty()
            || self.bot_name.is_empty()
            || self.priv_key.is_empty()
    }
}

impl Config {
    /// Create and return a new config.
    pub fn new() -> Result<(Self, bool), Box<dyn error::Error>> {
        let path = Path::new(&CONFIG_PATH).join(&CONFIG_FILE);
        println!("{}", path.display());

        if path.parent().is_some() && !path.parent().unwrap().exists() {
            create_dir_all(path.parent().unwrap())?;
        }

        let mut config_str = String::new();
        let mut oo = OpenOptions::new();

        let mut just_created = false;

        if path.exists() {
            oo.read(true).open(path)?.read_to_string(&mut config_str)?;
        } else {
            config_str = serde_yaml::to_string(&Config::default())?;
            oo.create(true)
                .write(true)
                .open(path)?
                .write_all(config_str.as_bytes())?;
            just_created = true;
        }

        Ok((from_str(&config_str)?, just_created))
    }

    /// Check if config is set up completely.
    pub fn need_adjustment(&self) -> bool {
        self.repo_dir.is_empty()
            || self.tmp_dir.is_empty()
            || self.rbuild.is_empty()
            || self.dmanager.is_empty()
            || self.git.is_empty()
    }

    /// Create all files needed for a working environment.
    pub fn create_environment(&self) -> Result<(), io::Error> {
        let tmp_path = Path::new(&self.tmp_dir);
        if !tmp_path.exists() {
            fs::create_dir(tmp_path)?;
        }

        Ok(())
    }

    /// Return a librb from a config
    pub fn as_rbuild(&self) -> librb::LibRb {
        librb::new(RequestConfig {
            machine_id: "".to_string(),
            username: self.rbuild.user_name.clone(),
            token: self.rbuild.token.clone(),
            url: self.rbuild.url.clone(),
        })
    }
}
