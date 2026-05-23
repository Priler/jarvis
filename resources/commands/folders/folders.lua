local id = jarvis.context.command_id

if id == "folder_music" then
    jarvis.system.open("F:\\temp\\jarvis_offline\\music")

elseif id == "folder_downloads" then
    jarvis.system.open("shell:Downloads")

elseif id == "folder_documents" then
    jarvis.system.open("shell:Personal")

elseif id == "folder_desktop" then
    jarvis.system.open("shell:Desktop")
end

return { chain = false }
