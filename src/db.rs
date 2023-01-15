use anyhow::Result;

use mysql_async::{Pool};
use spdlog::LoggerBuilder;

use crate::{settings::Settings};

pub async fn create_pool(
    mut builder: LoggerBuilder,
    settings: &Settings<'_>,
) -> Result<Pool> {
    let logger = builder.name("sql").build()?;
    let user = settings.try_get::<String>("network.SQL_LOGIN")?;
    let pass = settings.try_get::<String>("network.SQL_PASSWORD")?;
    let host = settings.try_get::<String>("network.SQL_HOST")?;
    let port = settings.try_get::<u16>("network.SQL_PORT")?;
    let db = settings.try_get::<String>("network.SQL_DATABASE")?;

    let pool = mysql_async::Pool::new(
        format!("mysql://{}:{}@{}:{}/{}", user, pass, host, port, db).as_ref(),
    );

    Ok(pool)
}
