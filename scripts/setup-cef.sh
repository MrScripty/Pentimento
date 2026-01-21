#!/bin/bash
# Setup CEF (Chromium Embedded Framework) binaries for Pentimento
#
# Downloads CEF binaries to ~/.cache/pentimento/cef if not already present.
# Sets CEF_PATH environment variable for the build system.

set -e

# CEF version - update as needed
# Find versions at: https://cef-builds.spotifycdn.com/index.html
CEF_VERSION="144.0.6+g5f7e671+chromium-144.0.7559.59"
CEF_PLATFORM="linux64"
CEF_TYPE="minimal"  # Use minimal distribution (no debug symbols)

CEF_DIR="${CEF_CACHE_DIR:-$HOME/.cache/pentimento/cef}"
CEF_ARCHIVE="cef_binary_${CEF_VERSION}_${CEF_PLATFORM}_${CEF_TYPE}.tar.bz2"
# URL-encode + as %2B for the download URL
CEF_ARCHIVE_ENCODED="${CEF_ARCHIVE//+/%2B}"
CEF_URL="https://cef-builds.spotifycdn.com/${CEF_ARCHIVE_ENCODED}"

# Check if CEF is already downloaded
if [ -d "$CEF_DIR/Release" ] && [ -f "$CEF_DIR/Release/libcef.so" ]; then
    echo "CEF binaries already present at $CEF_DIR"
else
    echo "Downloading CEF binaries..."
    echo "Version: $CEF_VERSION"
    echo "Archive: $CEF_ARCHIVE"

    # Clean up any failed previous downloads
    rm -rf "$CEF_DIR"
    mkdir -p "$CEF_DIR"

    # Download and extract
    if command -v curl &> /dev/null; then
        curl -L --progress-bar "$CEF_URL" | tar xj -C "$CEF_DIR" --strip-components=1
    elif command -v wget &> /dev/null; then
        wget -O - "$CEF_URL" | tar xj -C "$CEF_DIR" --strip-components=1
    else
        echo "Error: Neither curl nor wget found. Please install one of them."
        exit 1
    fi

    echo "CEF binaries downloaded to $CEF_DIR"
fi

# Export path for build system
export CEF_PATH="$CEF_DIR"
echo "CEF_PATH=$CEF_PATH"

# Verify the installation
if [ ! -f "$CEF_DIR/Release/libcef.so" ]; then
    echo "Error: CEF installation appears incomplete. Missing libcef.so"
    exit 1
fi

echo "CEF setup complete."
