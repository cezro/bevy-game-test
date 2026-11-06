@echo off
echo Starting ChronoStick...
if exist "target\debug\chrono_stick.exe" (
    start "" "target\debug\chrono_stick.exe"
) else (
    echo Binary not found. Building and running with Cargo...
    "%USERPROFILE%\.cargo\bin\cargo.exe" run
)
