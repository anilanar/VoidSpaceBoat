use itertools::Itertools;
use std::collections::HashMap;
use std::str;

use super::error::ServerError;
use super::lua::Lua;
use mlua::Value;

#[derive(Debug)]
pub struct Settings<'lua> {
    settings: HashMap<String, Value<'lua>>,
}

#[derive(Debug)]
pub enum Error {
    ParseValueError { key: String },
}

impl<'lua> Settings<'lua> {
    pub fn new(lua: &'lua Lua) -> Result<Settings, ServerError> {
        // load default settings
        load_lua_from_dir(lua, "settings/default")?;

        // load user settings
        load_lua_from_dir(lua, "settings")?;

        // load settings from env vars
        apply_env_variables(lua)?;

        let settings = populate_hashmap(lua)?;

        Ok(Settings { settings })
    }

    pub fn get(self: &Self, key: &str) -> Option<&Value<'lua>> {
        self.settings.get(key)
    }
}

/// Reads all lua files in the given directory and loads them into `lua`, sorted by name. Ignores non-lua files, if any.
fn load_lua_from_dir<P: AsRef<std::path::Path>>(
    lua: &Lua,
    path: P,
) -> Result<(), ServerError> {
    let root = std::env::current_dir().map_err(ServerError::IOError)?;

    let mut paths = Vec::new();

    for entry in
        std::fs::read_dir(root.join(path)).map_err(ServerError::IOError)?
    {
        let entry = entry.map_err(ServerError::IOError)?;
        let path = entry.path();
        let is_lua = path.extension().map(|ext| ext == "lua").unwrap_or(false);
        if is_lua {
            paths.push(path);
        }
    }

    paths.sort();

    for path in paths {
        lua.execute_file(&path)?;
    }

    Ok(())
}

/// Reads `xi.settings` from lua env, assumes a 2-level table hierarchy and
/// populates a hash map.
///
/// For example, if `xi.settings.foo.bar = 5`, then the hash map will contain
/// `("foo.bar", SettingsValue::Int(5))` pair.
fn populate_hashmap(lua: &Lua) -> Result<HashMap<String, Value>, ServerError> {
    let table = lua
        .globals()
        .get::<_, mlua::Table>("xi")
        .and_then(|table| table.get::<_, mlua::Table>("settings"))
        .map_err(ServerError::LuaError)?;

    let mut settings = HashMap::<String, Value>::new();

    for outer_entry in table.pairs::<String, mlua::Table>() {
        let (outer_key, outer_value) =
            outer_entry.map_err(ServerError::LuaError)?;

        for inner_entry in outer_value.pairs::<String, Value>() {
            let (inner_key, inner_value) =
                inner_entry.map_err(|err| match err {
                    mlua::Error::FromLuaConversionError {
                        from: _,
                        to: _,
                        message: _,
                    } => ServerError::SettingsError(Error::ParseValueError {
                        key: format!("{}.{}", outer_key, "?"),
                    }),
                    _ => ServerError::LuaError(err),
                })?;

            settings
                .insert(format!("{}.{}", outer_key, inner_key), inner_value);
        }
    }

    Ok(settings)
}

/// Finds env variables of the format `XI_a_b`, then proceeeds to add
/// sets `xi.settings.a.b` to the relevant value.
fn apply_env_variables(lua: &Lua) -> Result<(), ServerError> {
    // lua indices start at 1
    let mut idx: usize = 1;
    let mut code: String = String::new();
    let mut values: Vec<mlua::Value> = Vec::new();

    for (k, v) in std::env::vars() {
        let mut split = k.split('_');
        (|| {
            let xi = split.next()?;
            if xi != "XI" {
                return None::<()>;
            }
            let outer = split.next()?.to_lowercase();
            let inner = split.join("_");

            code.push_str(&format!(
                "xi.settings.{}.{} = values[{}];\n",
                outer, inner, idx
            ));

            values.push(str_to_value(lua, &v).ok()?);

            idx += 1;

            None
        })();
    }

    let fn_whole = format!(
        r#"
            function (values)
                {}
            end
        "#,
        code
    );

    lua.mlua()
        .load(&fn_whole)
        .eval::<mlua::Function>()
        .map_err(ServerError::LuaError)?
        .call::<_, ()>(values)
        .map_err(ServerError::LuaError)
}

#[cfg(test)]

mod tests {
    use std::ffi::OsString;

    use super::Settings;
    use crate::lua::Lua;
    use envtestkit::lock::lock_test;
    use envtestkit::set_env;
    use mlua::Value;

    #[test]
    fn it_executes_lua() {
        let lua = Lua::new().unwrap();
        Settings::new(&lua).unwrap();
        let value: String = lua
            .eval(&"xi.settings.main.SERVER_NAME".to_owned())
            .unwrap();
        assert_eq!(value, "Nameless");
    }

    #[test]
    fn it_loads_settings() {
        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();
        let value = settings.get("main.SERVER_NAME").unwrap();
        assert_eq!(
            value,
            &Value::String(lua.mlua().create_string("Nameless").unwrap())
        );
    }

    #[test]
    fn it_loads_int_settings() {
        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();
        let value = settings.get("main.RIVERNE_PORTERS").unwrap();
        assert_eq!(value, &Value::Integer(120));
    }

    #[test]
    fn it_loads_bool_settings() {
        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();
        let value = settings
            .get("main.USE_ADOULIN_WEAPON_SKILL_CHANGES")
            .unwrap();
        assert_eq!(value, &Value::Boolean(true));
    }

    #[test]
    fn it_loads_float_settings() {
        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();
        let value = settings.get("main.CASKET_DROP_RATE").unwrap();
        assert_eq!(value, &Value::Number(0.1));
    }

    #[test]
    fn it_loads_int_env_var_into_lua() {
        let _lock = lock_test();
        let _env = set_env(OsString::from("XI_MAIN_FOO_BAR"), "9999");

        let lua = Lua::new().unwrap();
        Settings::new(&lua).unwrap();

        let value: i64 =
            lua.eval(&"xi.settings.main.FOO_BAR".to_owned()).unwrap();
        assert_eq!(value, 9999);
    }

    #[test]
    fn it_loads_int_env_var() {
        let _lock = lock_test();
        let _env = set_env(OsString::from("XI_MAIN_FOO_BAR"), "9999");

        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();

        let value = settings.get("main.FOO_BAR").unwrap();
        assert_eq!(value, &Value::Integer(9999));
    }

    #[test]
    fn it_loads_bool_env_var() {
        let _lock = lock_test();
        let _env = set_env(OsString::from("XI_MAIN_FOO_BAR"), "false");

        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();

        let value = settings.get("main.FOO_BAR").unwrap();
        assert_eq!(value, &Value::Boolean(false));
    }
}

fn str_to_value<'lua>(
    lua: &'lua Lua,
    s: &str,
) -> Result<Value<'lua>, ServerError> {
    Ok(s.parse::<i64>()
        .map(Value::Integer)
        .ok()
        .or_else(|| s.parse::<f64>().map(Value::Number).ok())
        .or_else(|| s.parse::<bool>().map(Value::Boolean).ok())
        .unwrap_or(Value::String(
            lua.mlua().create_string(s).map_err(ServerError::LuaError)?,
        )))
}
