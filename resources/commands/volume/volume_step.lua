local id = jarvis.context.command_id
local key = (id == "volume_up") and 175 or 174  -- VK_VOLUME_UP=175, VK_VOLUME_DOWN=174

local cmd = string.format(
    "powershell -NoProfile -c \"" ..
    "Add-Type -TypeDefinition 'using System;using System.Runtime.InteropServices;" ..
    "public class K{[DllImport(\\\"user32.dll\\\")]public static extern void keybd_event(byte v,byte s,int f,int e);}'; " ..
    "for($i=0;$i-lt5;$i++){[K]::keybd_event(%d,0,0,0);[K]::keybd_event(%d,0,2,0);Start-Sleep -Milliseconds 30}\"",
    key, key
)
jarvis.system.exec(cmd)
return { chain = false }
