use lib_remotebuild_rs::{config::RequestConfig, librb};
use serde::{Deserialize, Serialize};
use serde_yaml::from_str;

use std::error;
use std::fs;
use std::fs::{create_dir_all, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

const CONFIG_PATH: &str = "./data/config.yaml";

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Config {
    pub repo_dir: String,
    pub tmp_dir: String,
    pub rbuild: Rbuild,
    pub git: Git,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Git {
    pub url: String,
    pub user: String,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Rbuild {
    pub user_name: String,
    pub token: String,
    pub url: String,
}

impl Config {
    pub fn new() -> Result<(Self, bool), Box<dyn error::Error>> {
        let path = Path::new(&CONFIG_PATH);

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

    pub fn need_adjustment(&self) -> bool {
        self.repo_dir.is_empty()
            || self.tmp_dir.is_empty()
            || self.rbuild.user_name.is_empty()
            || self.rbuild.token.is_empty()
            || self.rbuild.url.is_empty()
            || self.git.url.is_empty()
            || self.git.user.is_empty()
    }

    pub fn create_environment(&self) -> Result<(), io::Error> {
        let tmp_path = Path::new(&self.tmp_dir);
        if !tmp_path.exists() {
            fs::create_dir(tmp_path)?;
        }

        Ok(())
    }

    pub fn as_rbuild(&self) -> librb::LibRb {
        librb::new(RequestConfig {
            machine_id: "".to_string(),
            username: self.rbuild.user_name.clone(),
            token: self.rbuild.token.clone(),
            url: self.rbuild.url.clone(),
        })
    }
}
