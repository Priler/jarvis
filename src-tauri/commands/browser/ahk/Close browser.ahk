; Rerun as admin, if required
If Not A_IsAdmin
{
    Run *RunAs "%A_ScriptFullPath%"
    ExitApp
}

; set partial title matching mode
SetTitleMatchMode, 2

; list of all browsers to close
GroupAdd, browsers, ahk_class MozillaWindowClass
GroupAdd, browsers, ahk_class IEFrame
GroupAdd, browsers, ahk_exe msedge.exe
GroupAdd, browsers, ahk_exe chrome.exe
GroupAdd, browsers, ahk_exe firefox.exe

; kill them all
Winclose, ahk_group browsers