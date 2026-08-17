@echo off
cd /d "%~dp0"
if not exist "node_modules" (
    echo Installazione dipendenze...
    call npm install
)
echo Avvio OmniRID Desktop...
start "" npx electron .
echo Fatto.

