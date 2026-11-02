// HTTP Lua API: GET, POST, JSON requests

use mlua::{Lua, Table, Value};
use std::collections::HashMap;
use std::time::Duration;

pub fn register(lua: &Lua, jarvis: &Table) -> mlua::Result<()> {
    let http = lua.create_table()?;

    // jarvis.http.get(url, headers?)
    let get_fn = lua.create_function(|lua, (url, headers): (String, Option<Table>)| {
        http_request(lua, "GET", &url, None, headers)
    })?;
    http.set("get", get_fn)?;

    // jarvis.http.post(url, body, headers?)
    let post_fn = lua.create_function(
        |lua, (url, body, headers): (String, Option<String>, Option<Table>)| {
            http_request(lua, "POST", &url, body, headers)
        },
    )?;
    http.set("post", post_fn)?;

    // jarvis.http.post_json(url, data, headers?)
    let post_json_fn = lua.create_function(
        |lua, (url, data, headers): (String, Table, Option<Table>)| {
            // convert Lua table to JSON string
            let json_value = table_to_json(lua, data)?;
            let body = serde_json::to_string(&json_value)
                .map_err(|e| mlua::Error::runtime(e.to_string()))?;

            // add content-type header
            let mut header_map: HashMap<String, String> = HashMap::new();
            header_map.insert("Content-Type".to_string(), "application/json".to_string());

            if let Some(h) = headers {
                for pair in h.pairs::<String, String>() {
                    if let Ok((k, v)) = pair {
                        header_map.insert(k, v);
                    }
                }
            }

            http_request_with_headers(lua, "POST", &url, Some(body), header_map)
        },
    )?;
    http.set("post_json", post_json_fn)?;

    // jarvis.http.json(url) - GET + parse JSON
    let json_fn = lua.create_function(|lua, url: String| {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| mlua::Error::runtime(e.to_string()))?;

        let response = client
            .get(&url)
            .send()
            .map_err(|e| mlua::Error::runtime(e.to_string()))?;

        if response.status().is_success() {
            let json: serde_json::Value = response
                .json()
                .map_err(|e| mlua::Error::runtime(e.to_string()))?;
            json_to_lua(lua, json)
        } else {
            Ok(Value::Nil)
        }
    })?;
    http.set("json", json_fn)?;

    jarvis.set("http", http)?;

    Ok(())
}

fn http_request(
    lua: &Lua,
    method: &str,
    url: &str,
    body: Option<String>,
    headers: Option<Table>,
) -> mlua::Result<Table> {
    let header_map: HashMap<String, String> = if let Some(h) = headers {
        h.pairs::<String, String>().filter_map(|r| r.ok()).collect()
    } else {
        HashMap::new()
    };

    http_request_with_headers(lua, method, url, body, header_map)
}

fn http_request_with_headers(
    lua: &Lua,
    method: &str,
    url: &str,
    body: Option<String>,
    headers: HashMap<String, String>,
) -> mlua::Result<Table> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| mlua::Error::runtime(e.to_string()))?;

    let mut request = match method {
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        _ => client.get(url),
    };

    for (k, v) in headers {
        request = request.header(&k, &v);
    }

    if let Some(b) = body {
        request = request.body(b);
    }

    let result = lua.create_table()?;

    match request.send() {
        Ok(response) => {
            result.set("ok", response.status().is_success())?;
            result.set("status", response.status().as_u16())?;

            // get headers
            let headers_table = lua.create_table()?;
            for (name, value) in response.headers() {
                if let Ok(v) = value.to_str() {
                    headers_table.set(name.as_str(), v)?;
                }
            }
            result.set("headers", headers_table)?;

            // get body
            match response.text() {
                Ok(text) => result.set("body", text)?,
                Err(e) => result.set("body", format!("Error reading body: {}", e))?,
            }
        }
        Err(e) => {
            result.set("ok", false)?;
            result.set("status", 0)?;
            result.set("error", e.to_string())?;
            result.set("body", "")?;
        }
    }

    Ok(result)
}

// Convert Lua table to serde_json::Value
fn table_to_json(lua: &Lua, table: Table) -> mlua::Result<serde_json::Value> {
    use serde_json::{Map, Value as JsonValue};

    // check if it's an array (sequential integer keys starting from 1)
    let is_array = table
        .clone()
        .pairs::<i64, Value>()
        .filter_map(|r| r.ok())
        .enumerate()
        .all(|(i, (k, _))| k == (i + 1) as i64);

    if is_array && table.len()? > 0 {
        let arr: Vec<JsonValue> = table
            .sequence_values::<Value>()
            .filter_map(|r| r.ok())
            .map(|v| lua_to_json(lua, v))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(JsonValue::Array(arr))
    } else {
        let mut map = Map::new();
        for pair in table.pairs::<String, Value>() {
            let (k, v) = pair?;
            map.insert(k, lua_to_json(lua, v)?);
        }
        Ok(JsonValue::Object(map))
    }
}

// Convert Lua Value to serde_json::Value
fn lua_to_json(lua: &Lua, value: Value) -> mlua::Result<serde_json::Value> {
    use serde_json::{Number, Value as JsonValue};

    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Boolean(b) => Ok(JsonValue::Bool(b)),
        Value::Integer(i) => Ok(JsonValue::Number(Number::from(i))),
        Value::Number(n) => Number::from_f64(n)
            .map(JsonValue::Number)
            .ok_or_else(|| mlua::Error::runtime("Invalid float")),
        Value::String(s) => Ok(JsonValue::String(s.to_str()?.to_string())),
        Value::Table(t) => table_to_json(lua, t),
        _ => Err(mlua::Error::runtime("Unsupported type for JSON")),
    }
}

// Convert serde_json::Value to Lua Value
fn json_to_lua(lua: &Lua, json: serde_json::Value) -> mlua::Result<Value> {
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
                table.set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
        JsonValue::Object(map) => {
            let table = lua.create_table()?;
            for (k, v) in map {
                table.set(k, json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}
