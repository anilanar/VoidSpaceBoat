use mlua;

use super::error::ServerError;
use itertools::Itertools;

pub struct Lua {
    mlua: mlua::Lua,
}

impl Lua {
    pub fn new() -> Result<Lua, ServerError> {
        Lua::_new().map_err(ServerError::LuaError)
    }

    fn _new() -> Result<Lua, mlua::Error> {
        let mlua = mlua::Lua::new();

        mlua.load(
            r#"
        if not bit then bit = require('bit') end
        function __FILE__() return debug.getinfo(2, 'S').source end
        function __LINE__() return debug.getinfo(2, 'l').currentline end
        function __FUNC__() return debug.getinfo(2, 'n').name end
    "#,
        )
        .exec()?;

        let print =
            mlua.create_function(|_, args: mlua::Variadic<String>| {
                log::info!("{}", args.iter().format(" "));
                Ok(())
            })?;

        mlua.globals().set("print", print)?;

        mlua.load(r#"print("hello", "foo", "bar")"#).exec()?;

        Ok(Lua { mlua })
    }

    pub fn execute_file(
        self: &Lua,
        path: &std::path::PathBuf,
    ) -> Result<(), ServerError> {
        self.mlua.load(path).exec().map_err(ServerError::LuaError)?;
        Ok(())
    }

    pub fn globals(self: &Lua) -> mlua::Table {
        self.mlua.globals()
    }

    pub fn eval<'a, R: mlua::FromLuaMulti<'a>>(
        self: &'a Lua,
        code: &str,
    ) -> Result<R, ServerError> {
        self.mlua.load(code).eval().map_err(ServerError::LuaError)
    }
}
