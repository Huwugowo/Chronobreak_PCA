@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b %errorlevel%
cl /nologo /std:c11 /W4 nvenc_m4_layout_probe.c /Fe:nvenc_m4_layout_probe.exe
if errorlevel 1 exit /b %errorlevel%
nvenc_m4_layout_probe.exe
