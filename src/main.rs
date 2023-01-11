mod error;
mod lua;
mod server_timer;
mod settings;

use env_logger;
use error::ServerError;
use server_timer::ServerTimer;
use std::net;
use std::time;

fn main() -> Result<(), ServerError> {
    env_logger::init();
    let timer = ServerTimer::new();
    let lua = lua::Lua::new()?;
    let settings = settings::Settings::new(&lua)?;

    Ok(())
}
