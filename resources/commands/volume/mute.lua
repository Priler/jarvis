-- Toggle mute via Windows media key (VK_VOLUME_MUTE = 0xAD = char 173)
jarvis.system.exec("powershell -NoProfile -c \"(New-Object -ComObject WScript.Shell).SendKeys([char]173)\"")
return { chain = false }
