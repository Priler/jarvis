#NoEnv  ; Recommended for performance and compatibility with future AutoHotkey releases.
; #Warn  ; Enable warnings to assist with detecting common errors.
SendMode Input  ; Recommended for new scripts due to its superior speed and reliability.
SetWorkingDir %A_ScriptDir%  ; Ensures a consistent starting directory.

; Step 1. Close steam.
WinKill, AHK_exe Steam.exe
Process, Close, Steam.exe

; Step 2. Switch monitor profile.
RunWait C:\MonitorProfileSwitcher\MonitorSwitcher.exe -load:"C:\Users\Abraham\AppData\Roaming\MonitorSwitcher\Profiles\Gaming.xml"

Sleep, 7000

; Step 3. Switch audio device to LG TV.
Run, nircmd setdefaultsounddevice "LG TV" 1

; Step 4. Run Steam big picture.
Run, steam://open/bigpicture