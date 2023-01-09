use std::collections::HashMap;

use itertools::Itertools;

use super::error::ServerError;
use super::lua::Lua;

pub struct Settings {
    settings: HashMap<String, SettingsValue>,
}

/// Represents possible values a setting can take in settings lua files.
#[derive(Debug)]
pub enum SettingsValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    /// Represents non-standard utf8 strings which might contain FFXI specific
    /// byte sequences that are only understood by the FFXI client.
    BadString(Vec<u8>),
}

impl Settings {
    pub fn new(lua: &Lua) -> Result<Settings, ServerError> {
        // load default settings
        load_lua_from_dir(lua, "settings/default")?;

        // load user settings
        load_lua_from_dir(lua, "settings")?;

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

        for inner_entry in outer_value.pairs::<String, mlua::Value>() {
            let (inner_key, inner_value) =
                inner_entry.map_err(ServerError::LuaError)?;

            let value = match inner_value {
                mlua::Value::Boolean(bool) => Some(SettingsValue::Bool(bool)),
                mlua::Value::Integer(n) => Some(SettingsValue::Int(n)),
                mlua::Value::Number(n) => Some(SettingsValue::Float(n)),
                mlua::Value::String(s) => {
                    let bytes = s.as_bytes();
                    let value = String::from_utf8(bytes.to_owned())
                        .map(SettingsValue::String)
                        .unwrap_or_else(|_| {
                            SettingsValue::BadString(bytes.to_owned())
                        });
                    Some(value)
                }
                _ => None,
            };

            if let Some(value) = value {
                settings.insert(format!("{}.{}", outer_key, inner_key), value);
            }
        }
    }

    Ok(settings)
}

#[cfg(test)]

mod tests {
    use super::Settings;
    use crate::{lua::Lua, settings::SettingsValue};

    #[test]
    fn it_executes_lua() {
        let lua = Lua::new().unwrap();
        Settings::new(&lua).unwrap();
        let server_name: String = lua
            .eval(&"xi.settings.main.SERVER_NAME".to_owned())
            .unwrap();
        assert_eq!(server_name, "Nameless");
    }

    #[test]
    fn it_loads_settings() {
        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();
        let server_name = settings.get("main.SERVER_NAME").unwrap();
        assert_eq!(
            if let SettingsValue::String(name) = server_name {
                Some(name.to_owned())
            } else {
                None
            },
            Some("Nameless".to_string())
        );
    }

    #[test]
    fn it_loads_int_settings() {
        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();
        let value = settings.get("main.RIVERNE_PORTERS").unwrap();
        assert_eq!(
            if let SettingsValue::Int(value) = value {
                Some(value.to_owned())
            } else {
                None
            },
            Some(120)
        );
    }

    #[test]
    fn it_loads_bool_settings() {
        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();
        let value = settings
            .get("main.USE_ADOULIN_WEAPON_SKILL_CHANGES")
            .unwrap();
        assert_eq!(
            if let SettingsValue::Bool(value) = value {
                Some(value.to_owned())
            } else {
                None
            },
            Some(true)
        );
    }

    #[test]
    fn it_loads_float_settings() {
        let lua = Lua::new().unwrap();
        let settings = Settings::new(&lua).unwrap();
        let value = settings.get("main.CASKET_DROP_RATE").unwrap();
        assert_eq!(
            if let SettingsValue::Float(value) = value {
                Some(value.to_owned())
            } else {
                None
            },
            Some(0.1)
        );
    }
}
