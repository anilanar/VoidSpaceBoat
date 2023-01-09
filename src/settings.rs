use itertools::Itertools;
use mlua::ToLua;
use std::collections::HashMap;
use std::str;

use super::error::ServerError;
use super::lua::Lua;

#[derive(Debug)]
pub struct Settings {
    settings: HashMap<String, SettingsValue>,
}

#[derive(Debug)]
pub enum Error {
    ParseValueError { key: String },
}

/// Represents possible values a setting can take in settings lua files.
#[derive(Debug, PartialEq)]
pub enum SettingsValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    /// Represents non-standard utf8 strings which might contain FFXI specific
    /// byte sequences that are only understood by the FFXI client.
    BadString(Vec<u8>),
    Unknown,
}

impl SettingsValue {
    fn from_str(s: &str) -> Self {
        s.parse::<i64>()
            .map(Self::Int)
            .ok()
            .or_else(|| s.parse::<f64>().map(Self::Float).ok())
            .or_else(|| s.parse::<bool>().map(Self::Bool).ok())
            .unwrap_or(Self::String(s.to_owned()))
    }
}

impl<'lua> mlua::ToLua<'lua> for SettingsValue {
    fn to_lua(self, lua: &'lua mlua::Lua) -> mlua::Result<mlua::Value<'lua>> {
        match self {
            SettingsValue::Bool(n) => Ok(mlua::Value::Boolean(n)),
            SettingsValue::Int(n) => Ok(mlua::Value::Integer(n)),
            SettingsValue::Float(n) => Ok(mlua::Value::Number(n)),
            SettingsValue::String(n) => {
                Ok(mlua::Value::String(lua.create_string(&n)?))
            }
            SettingsValue::BadString(n) => {
                Ok(mlua::Value::String(lua.create_string(&n)?))
            }
            SettingsValue::Unknown => Err(mlua::Error::ToLuaConversionError {
                from: "SettingsValue::Unknown",
                to: "mlua::Value",
                message: None,
            }),
        }
    }
}

impl<'lua> mlua::FromLua<'lua> for SettingsValue {
    fn from_lua(
        lua_value: mlua::Value<'lua>,
        _lua: &'lua mlua::Lua,
    ) -> mlua::Result<Self> {
        match lua_value {
            mlua::Value::Boolean(n) => Ok(SettingsValue::Bool(n)),
            mlua::Value::Integer(n) => Ok(SettingsValue::Int(n)),
            mlua::Value::Number(n) => Ok(SettingsValue::Float(n)),
            mlua::Value::String(n) => {
                let bytes = n.as_bytes();
                Ok(String::from_utf8(bytes.to_owned())
                    .map(SettingsValue::String)
                    .unwrap_or_else(|_| {
                        SettingsValue::BadString(bytes.to_owned())
                    }))
            }
            _ => Ok(SettingsValue::Unknown),
        }
    }
}

impl Settings {
    pub fn new(lua: &Lua) -> Result<Settings, ServerError> {
        // load default settings
        load_lua_from_dir(lua, "settings/default")?;

        // load user settings
        load_lua_from_dir(lua, "settings")?;

        // load settings from env vars
        apply_env_variables(lua)?;

        let settings = populate_hashmap(lua)?;

        Ok(Settings { settings })
    }

    pub fn get(self: &Settings, key: &str) -> Option<&SettingsValue> {
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
fn populate_hashmap(
    lua: &Lua,
) -> Result<HashMap<String, SettingsValue>, ServerError> {
    let table = lua
        .globals()
        .get::<_, mlua::Table>("xi")
        .and_then(|table| table.get::<_, mlua::Table>("settings"))
        .map_err(ServerError::LuaError)?;

    let mut settings = HashMap::<String, SettingsValue>::new();

    for outer_entry in table.pairs::<String, mlua::Table>() {
        let (outer_key, outer_value) =
            outer_entry.map_err(ServerError::LuaError)?;

        for inner_entry in outer_value.pairs::<String, SettingsValue>() {
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

            values.push(SettingsValue::from_str(&v).to_lua(lua.mlua()).ok()?);

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
    use crate::{lua::Lua, settings::SettingsValue};
    use envtestkit::lock::lock_test;
    use envtestkit::set_env;

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
        assert_eq!(value, &SettingsValue::String("Nameless".to_string()));
    }

    #[test]
    fn it_loads_int_settings() {
        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();
        let value = settings.get("main.RIVERNE_PORTERS").unwrap();
        assert_eq!(value, &SettingsValue::Int(120));
    }

    #[test]
    fn it_loads_bool_settings() {
        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();
        let value = settings
            .get("main.USE_ADOULIN_WEAPON_SKILL_CHANGES")
            .unwrap();
        assert_eq!(value, &SettingsValue::Bool(true));
    }

    #[test]
    fn it_loads_float_settings() {
        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();
        let value = settings.get("main.CASKET_DROP_RATE").unwrap();
        assert_eq!(value, &SettingsValue::Float(0.1));
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
        assert_eq!(value, &SettingsValue::Int(9999));
    }

    #[test]
    fn it_loads_bool_env_var() {
        let _lock = lock_test();
        let _env = set_env(OsString::from("XI_MAIN_FOO_BAR"), "false");

        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();

        let value = settings.get("main.FOO_BAR").unwrap();
        assert_eq!(value, &SettingsValue::Bool(false));
    }
}
