local id = jarvis.context.command_id

if id == "win_minimize" then
    jarvis.system.exec("powershell -NoProfile -c \"(New-Object -ComObject Shell.Application).MinimizeAll()\"")

elseif id == "win_trash" then
    jarvis.system.exec("powershell -NoProfile -c \"Clear-RecycleBin -Force -ErrorAction SilentlyContinue\"")

elseif id == "win_screenshot" then
    -- Send PrintScreen key to copy full screen to clipboard
    jarvis.system.exec("powershell -NoProfile -c \"Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('{PRTSC}')\"")

elseif id == "win_lock" then
    -- Lock workstation via Windows API
    jarvis.system.exec("rundll32.exe user32.dll,LockWorkStation")

elseif id == "win_sleep" then
    -- Suspend (sleep) the system
    jarvis.system.exec("rundll32.exe powrprof.dll,SetSuspendState 0,1,0")

elseif id == "win_clipboard" then
    -- Open clipboard history (Win+V equivalent via settings)
    jarvis.system.open("ms-settings:clipboard")

elseif id == "win_language" then
    -- Switch keyboard layout via Alt+Shift
    jarvis.system.exec("powershell -NoProfile -c \"(New-Object -ComObject WScript.Shell).SendKeys('%+')\"")

elseif id == "win_display_off" then
    -- WM_SYSCOMMAND (0x112) + SC_MONITORPOWER (0xF170) + lParam 2 = off
    jarvis.system.exec(
        "powershell -NoProfile -c \"" ..
        "Add-Type -TypeDefinition 'using System;using System.Runtime.InteropServices;" ..
        "public class D{[DllImport(\\\"user32.dll\\\")]public static extern IntPtr " ..
        "SendMessage(IntPtr h,int m,int w,int l);}'; " ..
        "[D]::SendMessage([IntPtr](-1),0x112,0xF170,2)\""
    )

elseif id == "win_empty_clipboard" then
    jarvis.system.exec("powershell -NoProfile -c \"Set-Clipboard -Value $null\"")

elseif id == "win_open_downloads" then
    jarvis.system.open("shell:Downloads")

elseif id == "win_open_desktop" then
    jarvis.system.exec("powershell -NoProfile -c \"(New-Object -ComObject Shell.Application).ToggleDesktop()\"")

elseif id == "win_night_mode" then
    jarvis.system.open("ms-settings:nightlight")

end

return { chain = false }
