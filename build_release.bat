@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
cd /d "C:\Programing\PersonalAI\option2"
set PATH=C:\Users\bfipa\winlibs_tmp\mingw64\bin;%PATH%
npm run tauri build 2>&1
