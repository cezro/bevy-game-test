@echo off
setlocal
cd /d "%~dp0"
title OUT OF TIME - Game Launcher

echo ======================================================================
echo           OUT OF TIME: Stickman Chronos Shift Launcher
echo ======================================================================
echo.

:: 1. Terminate any previous instance to avoid file locks
taskkill /F /IM chrono_stick.exe >nul 2>&1

:: 2. Verify project root and essential assets
if not exist "Cargo.toml" (
    echo [ERROR] Cargo.toml not found. Please run this script from the project root.
    goto :error_exit
)

if not exist "assets\audio\slash.wav" (
    echo [WARNING] Audio assets not found in assets\audio. Sound effects may be missing.
)

:: 3. Locate Cargo / Rust
echo [1/3] Checking for Rust and Cargo...
set "CARGO_BIN="

where cargo.exe >nul 2>&1
if %errorlevel% equ 0 (
    for /f "delims=" %%I in ('where cargo.exe 2^>nul') do (
        if not defined CARGO_BIN set "CARGO_BIN=%%I"
    )
)

if not defined CARGO_BIN (
    if exist "%USERPROFILE%\.cargo\bin\cargo.exe" (
        set "CARGO_BIN=%USERPROFILE%\.cargo\bin\cargo.exe"
        set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
    )
)

if not defined CARGO_BIN (
    if exist "%CARGO_HOME%\bin\cargo.exe" (
        set "CARGO_BIN=%CARGO_HOME%\bin\cargo.exe"
        set "PATH=%CARGO_HOME%\bin;%PATH%"
    )
)

if defined CARGO_BIN goto :cargo_found

:: Fallback: Check if prebuilt binary exists before attempting install
echo [*] Cargo not detected on this system.
if exist "target\debug\chrono_stick.exe" (
    echo [INFO] Found precompiled game at target\debug\chrono_stick.exe
    echo Starting game directly...
    start "" "target\debug\chrono_stick.exe"
    exit /b 0
)
if exist "target\release\chrono_stick.exe" (
    echo [INFO] Found precompiled game at target\release\chrono_stick.exe
    echo Starting game directly...
    start "" "target\release\chrono_stick.exe"
    exit /b 0
)

:: No prebuilt binary and no cargo: install Rust
echo.
echo Rust is required to build the game, but is not installed.
echo Attempting automated Rust installation...
echo.

where winget.exe >nul 2>&1
if %errorlevel% equ 0 (
    echo Installing Rust via Windows Package Manager [winget]...
    winget install --id Rustlang.Rustup -e --silent --accept-package-agreements --accept-source-agreements
) else (
    echo Downloading rustup-init installer...
    powershell -NoProfile -ExecutionPolicy Bypass -Command "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; (New-Object System.Net.WebClient).DownloadFile('https://win.rustup.rs/x86_64', [System.IO.Path]::Combine($env:TEMP, 'rustup-init.exe'))"
    if exist "%TEMP%\rustup-init.exe" (
        echo Running rustup-init...
        "%TEMP%\rustup-init.exe" -y --default-toolchain stable
        del "%TEMP%\rustup-init.exe" >nul 2>&1
    ) else (
        echo [ERROR] Failed to download rustup-init.exe.
        echo Please install Rust from https://rustup.rs/ and re-run run.bat.
        goto :error_exit
    )
)

if exist "%USERPROFILE%\.cargo\bin\cargo.exe" (
    set "CARGO_BIN=%USERPROFILE%\.cargo\bin\cargo.exe"
    set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
) else (
    echo [ERROR] Cargo was not found at %USERPROFILE%\.cargo\bin after installation.
    echo Please restart your terminal or computer and re-run run.bat.
    goto :error_exit
)

:cargo_found
echo [OK] Cargo found: "%CARGO_BIN%"

:: 4. Check C++ Build Tools (MSVC Linker or GNU toolchain)
echo [2/3] Checking C++ build environment...
set "HAS_COMPILER=0"

where link.exe >nul 2>&1
if %errorlevel% equ 0 set "HAS_COMPILER=1"

if "%HAS_COMPILER%"=="0" (
    if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" (
        for /f "delims=" %%I in ('"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2^>nul') do (
            if exist "%%I" set "HAS_COMPILER=1"
        )
    )
)

if "%HAS_COMPILER%"=="0" (
    for /f "tokens=2 delims= " %%T in ('rustc -vV 2^>nul ^| findstr "host:"') do (
        set "RUST_HOST=%%T"
    )
    if defined RUST_HOST (
        echo %RUST_HOST% | findstr "gnu" >nul 2>&1
        if %errorlevel% equ 0 set "HAS_COMPILER=1"
    )
)

if "%HAS_COMPILER%"=="0" (
    echo [WARNING] Visual Studio C++ Build Tools not detected.
    echo Bevy requires MSVC Build Tools or a GNU toolchain to build on Windows.
    echo If compilation fails, install Visual Studio Build Tools via:
    echo   winget install Microsoft.VisualStudio.2022.BuildTools --override "--passive --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
    echo Or switch to GNU toolchain via:
    echo   rustup default stable-x86_64-pc-windows-gnu
    echo.
) else (
    echo [OK] C++ build tools detected.
)

:: 5. Compile and Run
echo [3/3] Compiling and starting ChronoStick...
echo.

"%CARGO_BIN%" run
if %errorlevel% equ 0 exit /b 0

echo.
echo ======================================================================
echo [ERROR] Cargo run failed with exit code %errorlevel%.

if exist "target\debug\chrono_stick.exe" (
    echo [FALLBACK] Launching existing build from target\debug\chrono_stick.exe...
    start "" "target\debug\chrono_stick.exe"
    exit /b 0
)
if exist "target\release\chrono_stick.exe" (
    echo [FALLBACK] Launching existing build from target\release\chrono_stick.exe...
    start "" "target\release\chrono_stick.exe"
    exit /b 0
)

:error_exit
echo ======================================================================
echo Press any key to exit...
pause >nul
exit /b 1
