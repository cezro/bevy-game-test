@echo off
taskkill /F /IM chrono_stick.exe >nul 2>&1
echo Building and running ChronoStick...
"%USERPROFILE%\.cargo\bin\cargo.exe" run
