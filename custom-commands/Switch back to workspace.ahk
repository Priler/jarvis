#NoEnv  ; Recommended for performance and compatibility with future AutoHotkey releases.
; #Warn  ; Enable warnings to assist with detecting common errors.
SendMode Input  ; Recommended for new scripts due to its superior speed and reliability.
SetWorkingDir %A_ScriptDir%  ; Ensures a consistent starting directory.

; Step 1. Close steam.
WinKill, AHK_exe Steam.exe
Process, Close, Steam.exe

; Step 2. Switch monitor profile.
RunWait C:\MonitorProfileSwitcher\MonitorSwitcher.exe -load:"C:\Users\Abraham\AppData\Roaming\MonitorSwitcher\Profiles\Workspace.xml"

Sleep, 7000

; Step 3. Switch audio device to Luna Edifier.
Run, nircmd setdefaultsounddevice "Luna Edifier" 1