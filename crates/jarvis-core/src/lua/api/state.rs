// State Lua API: persistent key-value storage per command

use mlua::{Lua, Result, Table, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const STATE_FILE: &str = ".state.json";

pub fn register(lua: &Lua, jarvis: &Table, command_path: &PathBuf) -> mlua::Result<()> {
    let state = lua.create_table()?;
    let state_path = command_path.join(STATE_FILE);

    // jarvis.state.get(key)
    let state_path_get = state_path.clone();
    let get_fn = lua.create_function(move |lua, key: String| {
        let data = load_state(&state_path_get);

        if let Some(value) = data.get(&key) {
            json_to_lua_value(lua, value.clone())
        } else {
            Ok(Value::Nil)
        }
    })?;
    state.set("get", get_fn)?;

    // jarvis.state.set(key, value)
    let state_path_set = state_path.clone();
    let set_fn = lua.create_function(move |_, (key, value): (String, Value)| {
        let mut data = load_state(&state_path_set);

        let json_value = lua_to_json_value(value)?;
        data.insert(key, json_value);

        save_state(&state_path_set, &data)?;
        Ok(true)
    })?;
    state.set("set", set_fn)?;

    // jarvis.state.delete(key)
    let state_path_delete = state_path.clone();
    let delete_fn = lua.create_function(move |_, key: String| {
        let mut data = load_state(&state_path_delete);
        let existed = data.remove(&key).is_some();
        save_state(&state_path_delete, &data)?;
        Ok(existed)
    })?;
    state.set("delete", delete_fn)?;

    // jarvis.state.clear()
    let state_path_clear = state_path.clone();
    let clear_fn = lua.create_function(move |_, ()| {
        let data: HashMap<String, serde_json::Value> = HashMap::new();
        save_state(&state_path_clear, &data)?;
        Ok(true)
    })?;
    state.set("clear", clear_fn)?;

    // jarvis.state.keys()
    let state_path_keys = state_path.clone();
    let keys_fn = lua.create_function(move |lua, ()| {
        let data = load_state(&state_path_keys);
        let table = lua.create_table()?;

        for (i, key) in data.keys().enumerate() {
            table.set(i + 1, key.clone())?;
        }

        Ok(table)
    })?;
    state.set("keys", keys_fn)?;

    // jarvis.state.all()
    let state_path_all = state_path.clone();
    let all_fn = lua.create_function(move |lua, ()| {
        let data = load_state(&state_path_all);
        let table = lua.create_table()?;

        for (key, value) in data {
            let lua_value = json_to_lua_value(lua, value)?;
            table.set(key, lua_value)?;
        }

        Ok(table)
    })?;
    state.set("all", all_fn)?;

    jarvis.set("state", state)?;

    Ok(())
}

fn load_state(path: &PathBuf) -> HashMap<String, serde_json::Value> {
    if !path.exists() {
        return HashMap::new();
    }

    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_state(path: &PathBuf, data: &HashMap<String, serde_json::Value>) -> mlua::Result<()> {
    let json =
        serde_json::to_string_pretty(data).map_err(|e| mlua::Error::runtime(e.to_string()))?;

    fs::write(path, json).map_err(|e| mlua::Error::runtime(e.to_string()))?;

    Ok(())
}

fn lua_to_json_value(value: Value) -> mlua::Result<serde_json::Value> {
    use serde_json::Value as JsonValue;

    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Boolean(b) => Ok(JsonValue::Bool(b)),
        Value::Integer(i) => Ok(JsonValue::Number(i.into())),
        Value::Number(n) => serde_json::Number::from_f64(n)
            .map(JsonValue::Number)
            .ok_or_else(|| mlua::Error::runtime("Invalid float")),
        Value::String(s) => Ok(JsonValue::String(s.to_str()?.to_string())),
        Value::Table(t) => {
            // check if array
            let is_array = t
                .clone()
                .pairs::<i64, Value>()
                .filter_map(|r| r.ok())
                .enumerate()
                .all(|(i, (k, _))| k == (i + 1) as i64);

            if is_array && t.len().unwrap_or(0) > 0 {
                let arr: Vec<JsonValue> = t
                    .sequence_values::<Value>()
                    .filter_map(|r| r.ok())
                    .map(lua_to_json_value)
                    .collect::<Result<Vec<_>>>()?;
                Ok(JsonValue::Array(arr))
            } else {
                let mut map = serde_json::Map::new();
                for pair in t.pairs::<String, Value>() {
                    let (k, v) = pair?;
                    map.insert(k, lua_to_json_value(v)?);
                }
                Ok(JsonValue::Object(map))
            }
        }
        _ => Err(mlua::Error::runtime("Unsupported type for state")),
    }
}

fn json_to_lua_value(lua: &Lua, json: serde_json::Value) -> mlua::Result<Value> {
    use serde_json::Value as JsonValue;

    match json {
        JsonValue::Null => Ok(Value::Nil),
        JsonValue::Bool(b) => Ok(Value::Boolean(b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Ok(Value::Nil)
            }
        }
        JsonValue::String(s) => Ok(Value::String(lua.create_string(&s)?)),
        JsonValue::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.into_iter().enumerate() {
                table.set(i + 1, json_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
        JsonValue::Object(map) => {
            let table = lua.create_table()?;
            for (k, v) in map {
                table.set(k, json_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}
