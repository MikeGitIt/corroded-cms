use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use crate::auth;

#[derive(Debug, Parser)]
#[command(version, about = "Corroded CMS server and maintenance CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Serve,
    Migrate,
    CreateAdmin {
        #[arg(long)]
        email: String,
        #[arg(long)]
        display_name: Option<String>,
        #[arg(long, env = "CORRODED_CMS_ADMIN_PASSWORD")]
        password: Option<String>,
    },
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

pub fn read_admin_password(provided: Option<String>) -> Result<String> {
    if let Some(password) = provided {
        return Ok(password);
    }

    let password = rpassword::prompt_password("Password: ").context("failed to read password")?;
    let confirmation =
        rpassword::prompt_password("Confirm password: ").context("failed to confirm password")?;

    if password != confirmation {
        bail!("password confirmation did not match");
    }

    Ok(password)
}

pub async fn create_admin(
    pool: &sqlx::PgPool,
    email: &str,
    display_name: Option<&str>,
    password: Option<String>,
) -> Result<()> {
    let password = read_admin_password(password)?;
    let display_name = display_name.unwrap_or(email);
    auth::create_admin(pool, email, display_name, &password).await
}
