local MUSIC_DIR = "F:\\temp\\jarvis_offline\\music"
local id = jarvis.context.command_id

local function send_media_key(key_code)
    -- keybd_event is required for VK codes above 127 (media keys 176-179).
    -- exec() takes (program, args_table) — pass powershell as the program
    -- and the script as a separate -c argument so the OS can find the binary.
    local script = string.format(
        "Add-Type -TypeDefinition 'using System;using System.Runtime.InteropServices;" ..
        "public class K{[DllImport(\"user32.dll\")]public static extern void keybd_event(byte v,byte s,int f,int e);}'; " ..
        "[K]::keybd_event(%d,0,0,0);[K]::keybd_event(%d,0,2,0)",
        key_code, key_code
    )
    jarvis.system.exec("powershell", {"-NoProfile", "-NonInteractive", "-c", script})
end

if id == "music_play" then
    local files = jarvis.fs.list(MUSIC_DIR)
    local tracks = {}
    for _, f in ipairs(files) do
        if f.is_file then
            local ext = f.name:lower():match("%.([^%.]+)$")
            if ext == "mp3" or ext == "wav" or ext == "flac" or ext == "m4a" or ext == "ogg" then
                table.insert(tracks, f.path)
            end
        end
    end
    if #tracks == 0 then
        jarvis.log("warn", "[music] No tracks found in " .. MUSIC_DIR)
        return { chain = false }
    end
    math.randomseed(tonumber(jarvis.context.time.timestamp))
    local track = tracks[math.random(#tracks)]
    jarvis.log("info", "[music] Playing: " .. track)
    jarvis.system.open(track)

elseif id == "music_stop" then
    -- exec(program, args_table): pass powershell as the program, script as -c arg.
    -- Concatenating everything into the first argument used to fail with "program not found"
    -- because Command::new() was looking up the full string as a binary path.
    jarvis.system.exec("powershell", {
        "-NoProfile", "-NonInteractive", "-c",
        "Stop-Process -Name Microsoft.Media.Player -Force -ErrorAction SilentlyContinue; " ..
        "Stop-Process -Name wmplayer    -Force -ErrorAction SilentlyContinue; " ..
        "Stop-Process -Name vlc         -Force -ErrorAction SilentlyContinue; " ..
        "Stop-Process -Name Music.UI    -Force -ErrorAction SilentlyContinue; " ..
        "Stop-Process -Name AIMP        -Force -ErrorAction SilentlyContinue; " ..
        "Stop-Process -Name foobar2000  -Force -ErrorAction SilentlyContinue; " ..
        "Stop-Process -Name PotPlayerMini64 -Force -ErrorAction SilentlyContinue; " ..
        "Stop-Process -Name winamp      -Force -ErrorAction SilentlyContinue"
    })

elseif id == "music_pause" then
    send_media_key(179)  -- VK_MEDIA_PLAY_PAUSE

elseif id == "music_next" then
    send_media_key(176)  -- VK_MEDIA_NEXT_TRACK

elseif id == "music_prev" then
    send_media_key(177)  -- VK_MEDIA_PREV_TRACK
end

return { chain = false }
