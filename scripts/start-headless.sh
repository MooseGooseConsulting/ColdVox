#!/usr/bin/env bash
set -euo pipefail

echo "Starting headless environment..."

# Cleanup existing instances if any
pkill -f "Xvfb.*:99" || true
pkill -f "fluxbox.*:99" || true
rm -f /tmp/.X99-lock /tmp/.X11-unix/X99

# Start Xvfb
Xvfb :99 -screen 0 1024x768x24 > /dev/null 2>&1 &
echo "Xvfb started on :99"

# Wait for Xvfb to be ready
for i in {1..50}; do
    if command -v xset >/dev/null 2>&1 && xset -q -display :99 > /dev/null 2>&1; then
        break
    fi
    sleep 0.1
done

# Start fluxbox
if command -v fluxbox >/dev/null 2>&1; then
    fluxbox -display :99 > /dev/null 2>&1 &
    echo "fluxbox started on :99"
else
    echo "fluxbox not found, continuing without window manager"
fi
