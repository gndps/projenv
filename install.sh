#!/usr/bin/env bash
set -euo pipefail

REPO="gndps/projenv"
INSTALL_DIR="/usr/local/bin"
BINARY_NAME="projenv"

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Darwin) OS_NAME="apple-darwin" ;;
    Linux)  OS_NAME="unknown-linux-gnu" ;;
    *)
        echo "Unsupported OS: $OS" >&2
        exit 1
        ;;
esac

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
    arm64|aarch64) ARCH_NAME="aarch64" ;;
    x86_64)        ARCH_NAME="x86_64" ;;
    *)
        echo "Unsupported architecture: $ARCH" >&2
        exit 1
        ;;
esac

TARGET="${ARCH_NAME}-${OS_NAME}"
TARBALL="${BINARY_NAME}-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${TARBALL}"

echo "Detected platform: ${TARGET}"
echo "Downloading ${BINARY_NAME} from ${DOWNLOAD_URL} ..."

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

# Download the tarball
if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$DOWNLOAD_URL" -o "${TMP_DIR}/${TARBALL}"
elif command -v wget >/dev/null 2>&1; then
    wget -q "$DOWNLOAD_URL" -O "${TMP_DIR}/${TARBALL}"
else
    echo "Error: neither curl nor wget found. Please install one of them." >&2
    exit 1
fi

# Extract the tarball
echo "Extracting ..."
tar -xzf "${TMP_DIR}/${TARBALL}" -C "$TMP_DIR"

# Find the binary (may be nested in a subdirectory)
BINARY_PATH="$(find "$TMP_DIR" -type f -name "$BINARY_NAME" | head -1)"
if [[ -z "$BINARY_PATH" ]]; then
    echo "Error: could not find '${BINARY_NAME}' binary in the extracted archive." >&2
    exit 1
fi

# Install the binary
echo "Installing to ${INSTALL_DIR}/${BINARY_NAME} ..."
if [[ -w "$INSTALL_DIR" ]]; then
    cp "$BINARY_PATH" "${INSTALL_DIR}/${BINARY_NAME}"
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}"
else
    sudo cp "$BINARY_PATH" "${INSTALL_DIR}/${BINARY_NAME}"
    sudo chmod +x "${INSTALL_DIR}/${BINARY_NAME}"
fi

echo "Successfully installed ${BINARY_NAME} to ${INSTALL_DIR}/${BINARY_NAME}"
echo ""
echo "Add shell integration by adding to your ~/.bashrc or ~/.zshrc:"
echo '  eval "$(projenv init-shell)"'
