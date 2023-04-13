#NoEnv  ; Recommended for performance and compatibility with future AutoHotkey releases.
; #Warn  ; Enable warnings to assist with detecting common errors.
SendMode Input  ; Recommended for new scripts due to its superior speed and reliability.
SetWorkingDir %A_ScriptDir%  ; Ensures a consistent starting directory.

Process, Exist, Y.Music.exe
If ErrorLevel = 0
{
; APP IS NOT RUNNING
; Run https://music.yandex.ru/home
Run C:\Users\Abraham\Documents\WinStoreApps\YandexMusic
; Autoplay
sleep 3000
Send, {Ctrl down}p{Ctrl up}
sleep 10

; Open full
Loop, 4
{
Send, {Tab}
sleep 10
}

Send, {Enter}
sleep 1500

Loop, 7
{
Send, {Down}
sleep 50
}

sleep 1000
Send, {Enter}
}
Else
{
; APP IS RUNNING
;MsgBox, Already running
}
Return