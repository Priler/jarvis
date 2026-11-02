// Audio Lua API: something sound related, apparently :3

use crate::voices::{self, Reaction};
use mlua::{Lua, Table};

pub fn register(lua: &Lua, jarvis: &Table) -> mlua::Result<()> {
    let audio = lua.create_table()?;

    // jarvis.audio.play(reaction)
    // reactions: "ok", "reply", "greet", "not_found", "error", "goodbye", "thanks"
    let play_fn = lua.create_function(|_, reaction: String| {
        let reaction_enum = match reaction.to_lowercase().as_str() {
            "ok" => Reaction::Ok,
            "reply" => Reaction::Reply,
            "greet" => Reaction::Greet,
            "not_found" => Reaction::NotFound,
            "error" => Reaction::Error,
            "goodbye" => Reaction::Goodbye,
            "thanks" => Reaction::Thanks,
            // "joke" => Reaction::Joke, NO PUN INTENDED :3
            _ => {
                log::warn!("[Lua] Unknown reaction: {}", reaction);
                return Ok(false);
            }
        };

        voices::play(reaction_enum);
        Ok(true)
    })?;
    audio.set("play", play_fn)?;

    // jarvis.audio.play_ok()
    let play_ok_fn = lua.create_function(|_, ()| {
        voices::play_ok();
        Ok(())
    })?;
    audio.set("play_ok", play_ok_fn)?;

    // jarvis.audio.play_reply()
    let play_reply_fn = lua.create_function(|_, ()| {
        voices::play_reply();
        Ok(())
    })?;
    audio.set("play_reply", play_reply_fn)?;

    // jarvis.audio.play_error()
    let play_error_fn = lua.create_function(|_, ()| {
        voices::play_error();
        Ok(())
    })?;
    audio.set("play_error", play_error_fn)?;

    // jarvis.audio.play_not_found()
    let play_not_found_fn = lua.create_function(|_, ()| {
        voices::play_not_found();
        Ok(())
    })?;
    audio.set("play_not_found", play_not_found_fn)?;

    // jarvis.audio.play_greet()
    let play_greet_fn = lua.create_function(|_, ()| {
        voices::play_greet();
        Ok(())
    })?;
    audio.set("play_greet", play_greet_fn)?;

    // jarvis.audio.play_goodbye()
    let play_goodbye_fn = lua.create_function(|_, ()| {
        voices::play_goodbye();
        Ok(())
    })?;
    audio.set("play_goodbye", play_goodbye_fn)?;

    jarvis.set("audio", audio)?;

    Ok(())
}
