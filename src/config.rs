use core::fmt;
use std::error::Error;
use std::{error, fs};

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub network: Network,
}

#[derive(Deserialize)]
pub struct Network {
    pub ip: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug)]
pub enum ConfigError {
    FileNotFound,
    MalformedConfig(toml::de::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ConfigError::FileNotFound => {
                write!(f, "could not found the 'config.toml' config file")
            }
            ConfigError::MalformedConfig(ref e) => {
                write!(f, "{e}")
            }
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            ConfigError::FileNotFound => None,
            ConfigError::MalformedConfig(ref e) => Some(e),
        }
    }
}

pub fn read_config() -> Result<Config, ConfigError> {
    let config_raw = fs::read_to_string("config.toml").map_err(|_| ConfigError::FileNotFound)?;
    toml::from_str(&config_raw).map_err(|e| ConfigError::MalformedConfig(e))
}
