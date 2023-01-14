mod db;
mod error;
mod logging;
mod lua;
mod server_timer;
mod settings;
mod socket;

use std::{env::current_dir, future::Future};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use clap::Parser;
use env_logger;
use error::ServerError;
use server_timer::ServerTimer;
use settings::Settings;
use spdlog::prelude::*;

const LOGIN_ERROR: u8 = 0x02;
const LOGIN_ATTEMPT: u8 = 0x10;
const LOGIN_CREATE: u8 = 0x20;
const LOGIN_CHANGE_PASSWORD: u8 = 0x30;

#[derive(Parser)]
struct CliArgs {
    log: Option<std::path::PathBuf>,
    append_date: Option<bool>,
}

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    let cli_args = CliArgs::parse();

    let builder = logging::builder(
        cli_args.log.unwrap_or(
            current_dir()
                .map_err(ServerError::IOError)?
                .as_path()
                .join("log")
                .join("login-server.log"),
        ),
        cli_args.append_date.unwrap_or(false),
    )?;

    let logger = builder
        .clone()
        .name("login")
        .build()
        .map_err(ServerError::LoggerError)?;

    env_logger::init();
    let timer = ServerTimer::new();
    let lua = lua::Lua::new()?;
    let settings = Settings::new(&lua)?;
    let db = db::Db::connect(builder, &settings).await?;

    db.execute(
        r#"OPTIMIZE TABLE `accounts`,`accounts_banned`, 
        `accounts_sessions`, `chars`,`char_equip`, `char_inventory`, 
        `char_jobs`,`char_look`,`char_stats`, `char_vars`, `char_bazaar_msg`,
        `char_skills`, `char_titles`, `char_effects`, `char_exp`;"#
            .to_owned(),
    )
    .await?;

    if !settings.try_get::<bool>("login.ACCOUNT_CREATION")? {
        info!(
            logger: logger,
            "New account creation is currently disabled."
        );
    }

    if !settings.try_get::<bool>("login.CHARACTER_DELETION")? {
        info!(logger: logger, "Character deletion is currently disabled.");
    }

    do_init(&settings).await?;

    Ok(())
}

async fn do_init<'lua>(settings: &Settings<'lua>) -> Result<(), ServerError> {
    let listener = TcpListener::bind(format!(
        "{}:{}",
        settings.try_get::<String>("network.LOGIN_AUTH_IP")?,
        settings.try_get::<u16>("network.LOGIN_AUTH_PORT")?
    ))
    .await
    .map_err(ServerError::IOError)?;

    loop {
        let (mut socket, addr) =
            listener.accept().await.map_err(ServerError::IOError)?;

        if let Err(err) = handle(&mut socket).await {
            println!("Error: {:?}", err);
        }
    }
}

async fn handle(socket: &mut TcpStream) -> Result<(), ServerError> {
    let mut buffer: [u8; 33] = [0; 33];
    socket
        .read_exact(&mut buffer)
        .await
        .map_err(ServerError::IOError)?;

    let name = std::str::from_utf8(&buffer[0..16]).ok();
    let password = std::str::from_utf8(&buffer[16..32]).ok();
    let code = buffer[32];

    if let (Some(name), Some(password)) = (name, password) {
        process(code, name, password);
    } else {
        socket
            .write(&[LOGIN_ERROR])
            .await
            .map_err(ServerError::IOError)?;
    }

    Ok(())
}

fn process(code: u8, name: &str, password: &str) {
    match code {
        LOGIN_ATTEMPT => {}
        _ => {}
    }
}

fn attempt_login(name: &str, password: &str) {
    let query = r#"SELECT accounts.id,accounts.status 
                        FROM accounts 
                        WHERE accounts.login = '%s' 
                        AND accounts.password = PASSWORD('%s')"#;
}
