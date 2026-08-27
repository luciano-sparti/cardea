#!/usr/bin/env bash
# ==============================================================================
# Cardea Installer Script
# Works both as a remote one-liner (curl | bash) and as a local installer.
# ==============================================================================
set -euo pipefail

REPO="Luciano-Sparti/cardea"
BIN_NAME="cardea"

# Determine target directory (system-wide if root, user-local otherwise)
if [ "$(id -u)" -eq 0 ]; then
    INSTALL_PREFIX="/usr/local"
    SHARE_PREFIX="/usr/local/share"
else
    INSTALL_PREFIX="${HOME}/.local"
    SHARE_PREFIX="${HOME}/.local/share"
fi

BIN_DIR="${INSTALL_PREFIX}/bin"
MAN_DIR="${SHARE_PREFIX}/man/man1"
BASH_COMP_DIR="${SHARE_PREFIX}/bash-completion/completions"
ZSH_COMP_DIR="${SHARE_PREFIX}/zsh/site-functions"
FISH_COMP_DIR="${SHARE_PREFIX}/fish/vendor_completions.d"
APPS_DIR="${SHARE_PREFIX}/applications"

# Detect Architecture
detect_arch() {
    local arch
    arch="$(uname -m)"
    case "$arch" in
        x86_64|amd64)
            echo "x86_64-unknown-linux-gnu"
            ;;
        aarch64|arm64)
            echo "aarch64-unknown-linux-gnu"
            ;;
        *)
            echo "Unsupported architecture: $arch" >&2
            exit 1
            ;;
    esac
}

# Determine if running from within an unpacked release directory
is_local_dir() {
    [ -f "./cardea" ] && [ -d "./completions" ]
}

install_files() {
    local src_dir="$1"
    echo "==> Installing Cardea to ${INSTALL_PREFIX}..."

    mkdir -p "$BIN_DIR" "$MAN_DIR" "$BASH_COMP_DIR" "$ZSH_COMP_DIR" "$FISH_COMP_DIR" "$APPS_DIR"

    # Install binary
    install -m 755 "${src_dir}/cardea" "${BIN_DIR}/cardea"

    # Install man page
    if [ -f "${src_dir}/man/cardea.1" ]; then
        install -m 644 "${src_dir}/man/cardea.1" "${MAN_DIR}/cardea.1"
    elif [ -x "${src_dir}/cardea" ]; then
        "${src_dir}/cardea" --generate-manpage > "${MAN_DIR}/cardea.1" 2>/dev/null || true
    fi

    # Install shell completions
    if [ -d "${src_dir}/completions" ]; then
        [ -f "${src_dir}/completions/cardea.bash" ] && install -m 644 "${src_dir}/completions/cardea.bash" "${BASH_COMP_DIR}/cardea"
        [ -f "${src_dir}/completions/_cardea" ] && install -m 644 "${src_dir}/completions/_cardea" "${ZSH_COMP_DIR}/_cardea"
        [ -f "${src_dir}/completions/cardea.fish" ] && install -m 644 "${src_dir}/completions/cardea.fish" "${FISH_COMP_DIR}/cardea.fish"
    else
        "${src_dir}/cardea" --generate-completions bash > "${BASH_COMP_DIR}/cardea" 2>/dev/null || true
        "${src_dir}/cardea" --generate-completions zsh > "${ZSH_COMP_DIR}/_cardea" 2>/dev/null || true
        "${src_dir}/cardea" --generate-completions fish > "${FISH_COMP_DIR}/cardea.fish" 2>/dev/null || true
    fi

    # Install desktop entry
    if [ -f "${src_dir}/assets/cardea.desktop" ]; then
        install -m 644 "${src_dir}/assets/cardea.desktop" "${APPS_DIR}/cardea.desktop"
    elif [ -f "./assets/cardea.desktop" ]; then
        install -m 644 "./assets/cardea.desktop" "${APPS_DIR}/cardea.desktop"
    fi

    echo ""
    echo "🎉 Cardea installed successfully!"
    echo "   Binary:      ${BIN_DIR}/cardea"
    echo "   Man Page:    ${MAN_DIR}/cardea.1"
    echo "   Desktop:     ${APPS_DIR}/cardea.desktop"
    echo ""
    if ! echo "$PATH" | tr ':' '\n' | grep -qx "${BIN_DIR}"; then
        echo "⚠️  Note: ${BIN_DIR} is not in your PATH."
        echo "   Add this to your shell profile (~/.bashrc or ~/.zshrc):"
        echo "   export PATH=\"${BIN_DIR}:\$PATH\""
        echo ""
    fi
}

main() {
    if is_local_dir; then
        install_files "."
        return 0
    fi

    local target
    target="$(detect_arch)"

    echo "==> Fetching latest release of Cardea for ${target}..."
    local tmp_dir
    tmp_dir="$(mktemp -d)"
    trap 'rm -rf "$tmp_dir"' EXIT

    local tag
    tag="$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name":' | head -1 | cut -d '"' -f 4 || echo "v0.1.0")"
    if [ -z "$tag" ]; then
        tag="v0.1.0"
    fi

    local tarball="cardea-${tag}-${target}.tar.gz"
    local url="https://github.com/${REPO}/releases/download/${tag}/${tarball}"

    echo "==> Downloading ${url}..."
    if ! curl -sSL -f -o "${tmp_dir}/${tarball}" "$url"; then
        # Fallback to generic target name if versioned tarball name varies
        tarball="cardea-${target}.tar.gz"
        url="https://github.com/${REPO}/releases/download/${tag}/${tarball}"
        curl -sSL -f -o "${tmp_dir}/${tarball}" "$url"
    fi

    echo "==> Extracting..."
    tar -xzf "${tmp_dir}/${tarball}" -C "$tmp_dir"

    # Find the directory containing the cardea binary
    local extracted_dir
    extracted_dir="$(find "$tmp_dir" -type f -name "cardea" -exec dirname {} \; | head -1)"

    install_files "$extracted_dir"
}

main "$@"
