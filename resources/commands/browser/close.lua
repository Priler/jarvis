local browsers = { "chrome.exe", "msedge.exe", "firefox.exe", "opera.exe", "brave.exe" }
for _, proc in ipairs(browsers) do
    jarvis.system.exec("taskkill /F /IM " .. proc)
end
return { chain = false }
