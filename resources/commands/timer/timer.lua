local id = jarvis.context.command_id

if id == "time_say" then
    local t = jarvis.context.time
    local msg = string.format("Сейчас %02d:%02d", t.hour, t.minute)
    jarvis.system.notify("Jarvis", msg)

else
    local minutes = 5
    if id == "timer_10min" then minutes = 10
    elseif id == "timer_15min" then minutes = 15
    end
    local seconds = minutes * 60

    -- Spawn a detached PowerShell that waits then shows a message box.
    -- $env:JT is inherited by the child process so no quoting issues with Russian text.
    local cmd = string.format(
        "powershell -NoProfile -c \"" ..
        "$env:JT='Таймер %d мин'; " ..
        "Start-Process powershell " ..
        "'-NoProfile -WindowStyle Hidden -c " ..
        "\\\"Start-Sleep %d; " ..
        "Add-Type -AssemblyName System.Windows.Forms; " ..
        "[System.Windows.Forms.MessageBox]::Show($env:JT)\\\"' " ..
        "-WindowStyle Hidden\"",
        minutes, seconds
    )
    jarvis.system.exec(cmd)
end

return { chain = false }
