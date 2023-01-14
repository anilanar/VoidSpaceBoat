use mlua;

#[derive(Debug)]
pub enum ServerError {
    LuaError(mlua::Error),
    IOError(std::io::Error),
    SettingsError(super::settings::Error),
    LoggerError(spdlog::Error),
    DbError(mysql_async::Error),
}
