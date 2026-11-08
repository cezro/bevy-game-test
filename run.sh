#!/usr/bin/env bash
set -e

# Change directory to the project root
cd "$(dirname "$0")"

echo "======================================================================"
echo "          OUT OF TIME: Stickman Chronos Shift Launcher                "
echo "======================================================================"
echo ""

# Terminate existing instance if running
pkill -f chrono_stick 2>/dev/null || true

# Verify project structure
if [ ! -f "Cargo.toml" ]; then
    echo "[ERROR] Cargo.toml not found! Please run this script from the project root."
    exit 1
fi

if [ ! -f "assets/audio/slash.wav" ]; then
    echo "[WARNING] Game audio assets not found in assets/audio! Sound effects may be missing."
fi

# Locate Cargo
if ! command -v cargo &> /dev/null; then
    if [ -f "$HOME/.cargo/env" ]; then
        source "$HOME/.cargo/env"
    fi
fi

if ! command -v cargo &> /dev/null; then
    echo "[!] Cargo was not found on this system."
    if [ -f "target/debug/chrono_stick" ]; then
        echo "[INFO] Found precompiled binary at target/debug/chrono_stick! Running directly..."
        exec "./target/debug/chrono_stick"
    elif [ -f "target/release/chrono_stick" ]; then
        echo "[INFO] Found precompiled binary at target/release/chrono_stick! Running directly..."
        exec "./target/release/chrono_stick"
    fi

    echo "Attempting to install Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

echo "[OK] Cargo found: $(command -v cargo)"

# Check Linux Bevy native dependencies
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    echo "[INFO] Checking Linux native libraries for Bevy..."
    MISSING_PKGS=()
    if command -v dpkg &> /dev/null; then
        for pkg in libasound2-dev libudev-dev pkg-config g++; do
            if ! dpkg -s "$pkg" &> /dev/null; then
                MISSING_PKGS+=("$pkg")
            fi
        done
        if [ ${#MISSING_PKGS[@]} -gt 0 ]; then
            echo "[WARNING] Missing recommended Bevy development packages: ${MISSING_PKGS[*]}"
            echo "You can install them with: sudo apt-get update && sudo apt-get install -y ${MISSING_PKGS[*]}"
        fi
    fi
fi

echo "Compiling and starting ChronoStick..."
echo ""
cargo run "$@"
