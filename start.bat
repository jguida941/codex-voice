@echo off
REM Codex Voice - Windows Launcher
REM Double-click this file to start

REM Prompt user to select a project folder
set "psCommand="(new-object -COM 'Shell.Application').BrowseForFolder(0,'Select your project folder',0,0).self.path""
for /f "usebackq delims=" %%I in (`powershell %psCommand%`) do set "PROJECT_DIR=%%I"

REM If user cancelled, exit
if "%PROJECT_DIR%"=="" (
    echo No folder selected. Exiting.
    pause
    exit /b
)

REM Save the project directory for codex-voice
set CODEX_VOICE_CWD=%PROJECT_DIR%

REM Change to project directory
cd /d "%PROJECT_DIR%"

REM Run the CLI (assumes npm is in PATH)
echo Starting Codex Voice in %PROJECT_DIR%...
cd /d "%~dp0ts_cli"
npm start

pause
