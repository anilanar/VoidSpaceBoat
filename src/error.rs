use mlua;

#[derive(Debug)]
pub enum ServerError {
    LuaError(mlua::Error),
    IOError(std::io::Error),
}
