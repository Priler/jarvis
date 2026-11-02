// File System Lua API: read, write, list (sandboxed)

use mlua::{Lua, Table};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::lua::sandbox::SandboxLevel;

pub fn register(
    lua: &Lua,
    jarvis: &Table,
    command_path: &PathBuf,
    sandbox: SandboxLevel,
) -> mlua::Result<()> {
    let fs_table = lua.create_table()?;

    let cmd_path = command_path.clone();
    let sandbox_level = sandbox;

    // jarvis.fs.read(path)
    let cmd_path_read = cmd_path.clone();
    let read_fn = lua.create_function(move |_, path: String| {
        let full_path = resolve_path(&cmd_path_read, &path, sandbox_level)?;

        fs::read_to_string(&full_path)
            .map_err(|e| mlua::Error::runtime(format!("Read error: {}", e)))
    })?;
    fs_table.set("read", read_fn)?;

    // jarvis.fs.read_bytes(path)
    let cmd_path_read_bytes = cmd_path.clone();
    let read_bytes_fn = lua.create_function(move |lua, path: String| {
        let full_path = resolve_path(&cmd_path_read_bytes, &path, sandbox_level)?;

        let bytes =
            fs::read(&full_path).map_err(|e| mlua::Error::runtime(format!("Read error: {}", e)))?;

        Ok(lua.create_string(&bytes)?)
    })?;
    fs_table.set("read_bytes", read_bytes_fn)?;

    // jarvis.fs.write(path, content)
    let cmd_path_write = cmd_path.clone();
    let write_fn = lua.create_function(move |_, (path, content): (String, String)| {
        if !sandbox_level.allows_fs_write() {
            return Err(mlua::Error::runtime("Write not allowed in this sandbox"));
        }

        let full_path = resolve_path(&cmd_path_write, &path, sandbox_level)?;

        // ensure parent directory exists
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| mlua::Error::runtime(format!("Create dir error: {}", e)))?;
        }

        fs::write(&full_path, content)
            .map_err(|e| mlua::Error::runtime(format!("Write error: {}", e)))?;

        Ok(true)
    })?;
    fs_table.set("write", write_fn)?;

    // jarvis.fs.append(path, content)
    let cmd_path_append = cmd_path.clone();
    let append_fn = lua.create_function(move |_, (path, content): (String, String)| {
        if !sandbox_level.allows_fs_write() {
            return Err(mlua::Error::runtime("Write not allowed in this sandbox"));
        }

        let full_path = resolve_path(&cmd_path_append, &path, sandbox_level)?;

        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&full_path)
            .map_err(|e| mlua::Error::runtime(format!("Open error: {}", e)))?;

        file.write_all(content.as_bytes())
            .map_err(|e| mlua::Error::runtime(format!("Write error: {}", e)))?;

        Ok(true)
    })?;
    fs_table.set("append", append_fn)?;

    // jarvis.fs.exists(path)
    let cmd_path_exists = cmd_path.clone();
    let exists_fn = lua.create_function(move |_, path: String| {
        let full_path = resolve_path(&cmd_path_exists, &path, sandbox_level)?;
        Ok(full_path.exists())
    })?;
    fs_table.set("exists", exists_fn)?;

    // jarvis.fs.is_file(path)
    let cmd_path_is_file = cmd_path.clone();
    let is_file_fn = lua.create_function(move |_, path: String| {
        let full_path = resolve_path(&cmd_path_is_file, &path, sandbox_level)?;
        Ok(full_path.is_file())
    })?;
    fs_table.set("is_file", is_file_fn)?;

    // jarvis.fs.is_dir(path)
    let cmd_path_is_dir = cmd_path.clone();
    let is_dir_fn = lua.create_function(move |_, path: String| {
        let full_path = resolve_path(&cmd_path_is_dir, &path, sandbox_level)?;
        Ok(full_path.is_dir())
    })?;
    fs_table.set("is_dir", is_dir_fn)?;

    // jarvis.fs.list(path)
    let cmd_path_list = cmd_path.clone();
    let list_fn = lua.create_function(move |lua, path: Option<String>| {
        let full_path = if let Some(p) = path {
            resolve_path(&cmd_path_list, &p, sandbox_level)?
        } else {
            cmd_path_list.clone()
        };

        let result = lua.create_table()?;

        let entries = fs::read_dir(&full_path)
            .map_err(|e| mlua::Error::runtime(format!("List error: {}", e)))?;

        let mut idx = 1;
        for entry in entries {
            if let Ok(entry) = entry {
                let item = lua.create_table()?;
                item.set("name", entry.file_name().to_string_lossy().to_string())?;
                item.set("path", entry.path().to_string_lossy().to_string())?;
                item.set("is_file", entry.path().is_file())?;
                item.set("is_dir", entry.path().is_dir())?;

                result.set(idx, item)?;
                idx += 1;
            }
        }

        Ok(result)
    })?;
    fs_table.set("list", list_fn)?;

    // jarvis.fs.mkdir(path)
    let cmd_path_mkdir = cmd_path.clone();
    let mkdir_fn = lua.create_function(move |_, path: String| {
        if !sandbox_level.allows_fs_write() {
            return Err(mlua::Error::runtime("Write not allowed in this sandbox"));
        }

        let full_path = resolve_path(&cmd_path_mkdir, &path, sandbox_level)?;

        fs::create_dir_all(&full_path)
            .map_err(|e| mlua::Error::runtime(format!("Mkdir error: {}", e)))?;

        Ok(true)
    })?;
    fs_table.set("mkdir", mkdir_fn)?;

    // jarvis.fs.remove(path)
    let cmd_path_remove = cmd_path.clone();
    let remove_fn = lua.create_function(move |_, path: String| {
        if !sandbox_level.allows_fs_write() {
            return Err(mlua::Error::runtime("Write not allowed in this sandbox"));
        }

        let full_path = resolve_path(&cmd_path_remove, &path, sandbox_level)?;

        if full_path.is_dir() {
            fs::remove_dir_all(&full_path)
                .map_err(|e| mlua::Error::runtime(format!("Remove error: {}", e)))?;
        } else {
            fs::remove_file(&full_path)
                .map_err(|e| mlua::Error::runtime(format!("Remove error: {}", e)))?;
        }

        Ok(true)
    })?;
    fs_table.set("remove", remove_fn)?;

    jarvis.set("fs", fs_table)?;

    Ok(())
}

// Resolve path relative to command folder, with sandbox checks
fn resolve_path(
    command_path: &PathBuf,
    path: &str,
    sandbox: SandboxLevel,
) -> mlua::Result<PathBuf> {
    let path = Path::new(path);

    // if absolute path, check sandbox allows it
    if path.is_absolute() {
        if !sandbox.allows_expanded_paths() {
            return Err(mlua::Error::runtime(
                "Absolute paths not allowed in this sandbox",
            ));
        }
        return Ok(path.to_path_buf());
    }

    // relative path - resolve against command folder
    if !sandbox.allows_expanded_paths() {
        for component in path.components() {
            match component {
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(mlua::Error::runtime("Path escapes command folder"));
                }
                _ => {}
            }
        }
    }

    let resolved = command_path.join(path);

    // canonicalize to resolve ../ etc and check it's still within command folder
    let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());

    let cmd_canonical = command_path
        .canonicalize()
        .unwrap_or_else(|_| command_path.clone());

    if !sandbox.allows_expanded_paths() && !canonical.starts_with(&cmd_canonical) {
        return Err(mlua::Error::runtime("Path escapes command folder"));
    }

    Ok(resolved)
}
