#!/usr/bin/env bash
set -euo pipefail

# ==============================================================================
# Fenestra Release Packaging Script
# Generates release tarballs, man pages, shell completions, and .deb packages
# ==============================================================================

VERSION=$(grep '^version = ' Cargo.toml | head -1 | cut -d '"' -f 2)
TARGET="${1:-x86_64-unknown-linux-gnu}"
DIST_DIR="dist"
STAGING_DIR="dist/fenestra-v${VERSION}-${TARGET}"

echo "==> Building Fenestra v${VERSION} for target: ${TARGET}..."
if [ "$TARGET" = "$(rustc -vV | grep 'host:' | cut -d ' ' -f 2)" ]; then
    cargo build --release --bin fenestra
    BIN_PATH="target/release/fenestra"
else
    cross build --release --target "$TARGET" --bin fenestra
    BIN_PATH="target/${TARGET}/release/fenestra"
fi

echo "==> Preparing staging directory: ${STAGING_DIR}..."
rm -rf "$STAGING_DIR"
mkdir -p "${STAGING_DIR}/completions" "${STAGING_DIR}/man" "${STAGING_DIR}/assets"

# Copy binary & core docs
cp "$BIN_PATH" "${STAGING_DIR}/"
cp README.md LICENSE "${STAGING_DIR}/"
cp assets/fenestra.desktop "${STAGING_DIR}/assets/"

# Generate shell completions and man page using the compiled host binary
HOST_BIN="target/release/fenestra"
if [ ! -f "$HOST_BIN" ]; then
    cargo build --release --bin fenestra
fi

echo "==> Generating shell completions..."
"$HOST_BIN" --generate-completions bash > "${STAGING_DIR}/completions/fenestra.bash"
"$HOST_BIN" --generate-completions zsh > "${STAGING_DIR}/completions/_fenestra"
"$HOST_BIN" --generate-completions fish > "${STAGING_DIR}/completions/fenestra.fish"

echo "==> Generating man page..."
"$HOST_BIN" --generate-manpage > "${STAGING_DIR}/man/fenestra.1"

# Copy install script into archive
cp install.sh "${STAGING_DIR}/install.sh"
chmod +x "${STAGING_DIR}/install.sh"

echo "==> Creating release archive..."
mkdir -p "$DIST_DIR"
tar -czvf "${DIST_DIR}/fenestra-v${VERSION}-${TARGET}.tar.gz" -C "$DIST_DIR" "fenestra-v${VERSION}-${TARGET}"

# Generate SHA256 checksum
(cd "$DIST_DIR" && sha256sum "fenestra-v${VERSION}-${TARGET}.tar.gz" > "fenestra-v${VERSION}-${TARGET}.tar.gz.sha256")

echo "==> Successfully packaged: ${DIST_DIR}/fenestra-v${VERSION}-${TARGET}.tar.gz"
