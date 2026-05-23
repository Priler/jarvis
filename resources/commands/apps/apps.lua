local id = jarvis.context.command_id

if id == "app_settings" then
    jarvis.system.open("ms-settings:")

elseif id == "app_discord" then
    jarvis.system.open("discord://")

elseif id == "app_telegram" then
    jarvis.system.open("tg://")

elseif id == "app_obs" then
    jarvis.system.exec(
        "powershell -NoProfile -c \"" ..
        "if(Get-Command obs64 -ErrorAction SilentlyContinue)" ..
        "{Start-Process obs64}" ..
        "elseif(Test-Path 'C:\\Program Files\\obs-studio\\bin\\64bit\\obs64.exe')" ..
        "{Start-Process 'C:\\Program Files\\obs-studio\\bin\\64bit\\obs64.exe'}\""
    )
end

return { chain = false }
