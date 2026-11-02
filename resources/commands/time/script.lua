local t = jarvis.context.time
local text = string.format("Текущее время %s:%s", t.hour, t.minute)

jarvis.log("info", text)
jarvis.speak(text)

return false

