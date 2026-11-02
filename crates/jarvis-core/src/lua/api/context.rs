// Context Lua API: read-only command context

use crate::lua::CommandContext;
use mlua::{Lua, Table};

use crate::commands::SlotValue;

pub fn register(lua: &Lua, jarvis: &Table, ctx: &CommandContext) -> mlua::Result<()> {
    let context = lua.create_table()?;

    // read-only context values
    context.set("phrase", ctx.phrase.clone())?;
    context.set("command_id", ctx.command_id.clone())?;
    context.set(
        "command_path",
        ctx.command_path.to_string_lossy().to_string(),
    )?;
    context.set("language", ctx.language.clone())?;

    // time info
    let time = lua.create_table()?;
    let now = chrono::Local::now();
    time.set("year", now.format("%Y").to_string())?;
    time.set("month", now.format("%m").to_string())?;
    time.set("day", now.format("%d").to_string())?;
    time.set("hour", now.format("%H").to_string())?;
    time.set("minute", now.format("%M").to_string())?;
    time.set("second", now.format("%S").to_string())?;
    time.set("weekday", now.format("%A").to_string())?;
    time.set("timestamp", now.timestamp())?;
    context.set("time", time)?;

    // slots
    let slots_table = lua.create_table()?;
    if let Some(ref slots) = ctx.slots {
        for (name, value) in slots {
            match value {
                SlotValue::Text(t) => slots_table.set(name.as_str(), t.as_str())?,
                SlotValue::Number(n) => slots_table.set(name.as_str(), *n)?,
            }
        }
    }
    context.set("slots", slots_table)?;

    jarvis.set("context", context)?;

    Ok(())
}
