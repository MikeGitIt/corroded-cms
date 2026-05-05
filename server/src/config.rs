use std::{env, path::PathBuf};

use anyhow::{Context, Result, bail};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Environment {
    Development,
    Production,
    Test,
}

impl Environment {
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "development" | "dev" => Ok(Self::Development),
            "production" | "prod" => Ok(Self::Production),
            "test" => Ok(Self::Test),
            other => bail!("unsupported ENVIRONMENT value `{other}`"),
        }
    }

    pub fn is_production(&self) -> bool {
        matches!(self, Self::Production)
    }
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub base_url: String,
    pub session_secret: String,
    pub upload_dir: PathBuf,
    pub environment: Environment,
    pub site_name: String,
    pub site_description: String,
    pub host: String,
    pub port: u16,
    pub max_upload_bytes: u64,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let environment = Environment::parse(&required("ENVIRONMENT")?)?;
        let session_secret = required("SESSION_SECRET")?;
        if environment.is_production() && session_secret.as_bytes().len() < 32 {
            bail!("SESSION_SECRET must be at least 32 bytes in production");
        }

        Ok(Self {
            database_url: required("DATABASE_URL")?,
            base_url: required("BASE_URL")?,
            session_secret,
            upload_dir: PathBuf::from(required("UPLOAD_DIR")?),
            environment,
            site_name: required("SITE_NAME")?,
            site_description: required("SITE_DESCRIPTION")?,
            host: optional_string("HOST", "127.0.0.1")?,
            port: optional_u16("PORT", 3000)?,
            max_upload_bytes: optional_u64("MAX_UPLOAD_BYTES", 5 * 1024 * 1024)?,
        })
    }
}

fn required(name: &str) -> Result<String> {
    let value = env::var(name).with_context(|| format!("{name} is required"))?;
    let value = value.trim().to_owned();
    if value.is_empty() {
        bail!("{name} cannot be empty");
    }
    Ok(value)
}

fn optional_u64(name: &str, default: u64) -> Result<u64> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => value
            .parse::<u64>()
            .with_context(|| format!("{name} must be an unsigned integer")),
        _ => Ok(default),
    }
}

fn optional_u16(name: &str, default: u16) -> Result<u16> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => value
            .parse::<u16>()
            .with_context(|| format!("{name} must be a TCP port number")),
        _ => Ok(default),
    }
}

fn optional_string(name: &str, default: &str) -> Result<String> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value.trim().to_owned()),
        _ => Ok(default.to_owned()),
    }
}
