use std::time::Duration;

use mysql_async::{futures::GetConn, prelude::*, Conn, Pool};
use spdlog::LoggerBuilder;
use tokio::time::timeout;

use crate::{error::ServerError, settings::Settings};

pub struct Db {
    pool: Pool,
}

impl Db {
    pub async fn connect(
        mut builder: LoggerBuilder,
        settings: &Settings<'_>,
    ) -> Result<Db, ServerError> {
        let logger = builder
            .name("sql")
            .build()
            .map_err(ServerError::LoggerError)?;
        let user = settings.try_get::<String>("network.SQL_LOGIN")?;
        let pass = settings.try_get::<String>("network.SQL_PASSWORD")?;
        let host = settings.try_get::<String>("network.SQL_HOST")?;
        let port = settings.try_get::<u16>("network.SQL_PORT")?;
        let db = settings.try_get::<String>("network.SQL_DATABASE")?;

        let pool = mysql_async::Pool::new(
            format!("mysql://{}:{}@{}:{}/{}", user, pass, host, port, db)
                .as_ref(),
        );

        r#"OPTIMIZE TABLE `accounts`,`accounts_banned`, 
            `accounts_sessions`, `chars`,`char_equip`, `char_inventory`, 
            `char_jobs`,`char_look`,`char_stats`, `char_vars`, `char_bazaar_msg`,
            `char_skills`, `char_titles`, `char_effects`, `char_exp`;"#.ignore(conn).await.map_err(ServerError::DbError)?;

        Ok(Db { pool })
    }

    async fn get_conn(&self) -> Result<Conn, ServerError> {
        timeout(Duration::from_secs(3), self.pool.get_conn())
            .await
            .map_err(|err| {
                ServerError::DbError(mysql_async::Error::Other(Box::new(err)))
            })?
            .map_err(ServerError::DbError)
    }

    pub async fn execute(&self, query: String) -> Result<(), ServerError> {
        let conn = self.get_conn().await?;

        query.ignore(conn).await.map_err(ServerError::DbError)
    }
}
