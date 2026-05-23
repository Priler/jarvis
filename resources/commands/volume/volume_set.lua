-- Set system volume via PowerShell script.
-- Levels: volume_min=25%, volume_mid=50%, volume_max=100%
local id = jarvis.context.command_id
local level = "0.5"
if id == "volume_min" then level = "0.25"
elseif id == "volume_mid" then level = "0.5"
elseif id == "volume_max" then level = "1.0"
end

local ps1 = jarvis.context.command_path .. "\\set_volume.ps1"
local cmd = string.format("powershell -NoProfile -ExecutionPolicy Bypass -File \"%s\" %s", ps1, level)
local result = jarvis.system.exec(cmd)
if not result or not result.success then
    jarvis.log("warn", "Volume set may have failed: " .. (result and result.stderr or "unknown"))
end
return { chain = false }
