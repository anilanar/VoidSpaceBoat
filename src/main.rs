mod db;
mod logging;
mod login_sessions;
mod lua;
mod repl;
mod server_timer;
mod settings;
mod socket;

use std::env::current_dir;

use anyhow::Result;
use mysql_async::{prelude::*, Pool};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use clap::Parser;
use env_logger;
use server_timer::ServerTimer;
use settings::Settings;
use spdlog::prelude::*;

const LOGIN_ERROR: u8 = 0x02;
const LOGIN_ATTEMPT: u8 = 0x10;
const LOGIN_CREATE: u8 = 0x20;
const LOGIN_CHANGE_PASSWORD: u8 = 0x30;

const ACCOUNT_STATUS_CODE_NORMAL: u32 = 0x01;
const ACCOUNT_STATUS_CODE_BANNED: u32 = 0x02;

#[derive(Parser)]
struct CliArgs {
    log: Option<std::path::PathBuf>,
    append_date: Option<bool>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli_args = CliArgs::parse();

    let builder = logging::builder(
        cli_args.log.unwrap_or(
            current_dir()?
                .as_path()
                .join("log")
                .join("login-server.log"),
        ),
        cli_args.append_date.unwrap_or(false),
    )?;

    let logger = builder.clone().name("login").build()?;

    env_logger::init();
    let timer = ServerTimer::new();
    let lua = lua::Lua::new()?;
    let settings = Settings::new(&lua)?;
    let pool = db::create_pool(builder, &settings).await?;
    let login_sessions = login_sessions::LoginSessions::new();

    r#"OPTIMIZE TABLE `accounts`,`accounts_banned`, 
        `accounts_sessions`, `chars`,`char_equip`, `char_inventory`, 
        `char_jobs`,`char_look`,`char_stats`, `char_vars`, `char_bazaar_msg`,
        `char_skills`, `char_titles`, `char_effects`, `char_exp`"#
        .ignore(pool)
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

async fn do_init<'lua>(settings: &Settings<'lua>) -> Result<()> {
    let listener = TcpListener::bind(format!(
        "{}:{}",
        settings.try_get::<String>("network.LOGIN_AUTH_IP")?,
        settings.try_get::<u16>("network.LOGIN_AUTH_PORT")?
    ))
    .await?;

    loop {
        let (mut socket, addr) = listener.accept().await?;

        if let Err(err) = handle(&mut socket).await {
            println!("Error: {:?}", err);
        }
    }
}

async fn handle(socket: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 33] = [0; 33];
    socket.read_exact(&mut buffer).await?;

    let name = std::str::from_utf8(&buffer[0..16]).ok();
    let password = std::str::from_utf8(&buffer[16..32]).ok();
    let code = buffer[32];

    if let (Some(name), Some(password)) = (name, password) {
        process(code, name, password);
    } else {
        socket.write(&[LOGIN_ERROR]).await?;
    }

    Ok(())
}

fn process(code: u8, name: &str, password: &str) {
    match code {
        LOGIN_ATTEMPT => {}
        _ => {}
    }
}

struct Session {
    acc_id: u32,
    status: u32,
}

async fn attempt_login(conn: &Pool, name: &str, password: &str) -> Result<()> {
    let session: Option<(u32, u32)> = r#"SELECT accounts.id,accounts.status 
        FROM accounts 
        WHERE accounts.login = :name 
        AND accounts.password = PASSWORD(:password)"#
        .with(params! {
            name, password
        })
        .first(conn)
        // .map(conn, |(acc_id, status)| Session { acc_id, status })
        .await?;

    if let Some((acc_id, status)) = session {
        if status & ACCOUNT_STATUS_CODE_NORMAL > 0 {
            post_login(acc_id, conn).await;
        }
    }

    Ok(())
}

async fn post_login(acc_id: u32, conn: &Pool) -> Result<()> {
    r#"UPDATE accounts SET 
        accounts.timelastmodify = NULL 
        WHERE accounts.id = :acc_id"#
        .with(params! {
            acc_id
        })
        .ignore(conn)
        .await?;

    let x: Option<(u32, u64, u64)> = r#"SELECT charid, server_addr, server_port
        FROM accounts_sessions JOIN accounts
        ON accounts_sessions.accid = accounts.id
        WHERE accounts.id = :acc_id"#
        .with(params! {
            acc_id
        })
        .first(conn)
        .await?;

    Ok(())
}
