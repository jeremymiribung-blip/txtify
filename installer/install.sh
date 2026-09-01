#!/usr/bin/env sh
# txtify installer - curl | sh for Linux/macOS
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/example/txtify/main/installer/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/example/txtify/main/installer/install.sh | sh -s -- --help
#   curl -fsSL https://github.com/example/txtify/releases/latest/download/install.sh -o install.sh && sh install.sh --prefix ~/.local/bin
# Security: always inspect script before piping to sh (less install.sh)
#
# Environment variables:
#   TXTIFY_VERSION - version to install (default: latest)
#   TXTIFY_REPO - GitHub repo (default: example/txtify)
#   TXTIFY_PREFIX - install directory (default: ~/.local/bin, fallback /usr/local/bin if writable)
#   TXTIFY_NO_MODIFY_PATH - if set, do not modify shell rc files

set -eu

REPO="${TXTIFY_REPO:-example/txtify}"
VERSION="${TXTIFY_VERSION:-latest}"
PREFIX="${TXTIFY_PREFIX:-}"
NO_MODIFY_PATH="${TXTIFY_NO_MODIFY_PATH:-}"
GITHUB_API="https://api.github.com/repos/${REPO}/releases"
GITHUB_RELEASES="https://github.com/${REPO}/releases"

# Parse args
for arg in "$@"; do
  case "$arg" in
    --help|-h)
      cat <<'EOF'
txtify installer (Linux/macOS)

Usage: sh install.sh [OPTIONS]

Options:
  --help, -h          Show this help
  --version VERSION   Install specific version (e.g. v0.1.0, default: latest)
  --prefix DIR        Install directory (default: ~/.local/bin)
  --no-modify-path    Do not modify shell rc files
  --force             Overwrite existing binary
  --from-source       Build from source via cargo (requires Rust)

Environment:
  TXTIFY_VERSION, TXTIFY_REPO, TXTIFY_PREFIX, TXTIFY_NO_MODIFY_PATH

Examples:
  curl -fsSL https://raw.githubusercontent.com/example/txtify/main/installer/install.sh | sh
  TXTIFY_VERSION=v0.1.0 sh install.sh
  sh install.sh --prefix ~/.cargo/bin
EOF
      exit 0
      ;;
    --version) shift; VERSION="$1" 2>/dev/null || true ;;
    --version=*) VERSION="${arg#--version=}" ;;
    --prefix) shift; PREFIX="$1" 2>/dev/null || true ;;
    --prefix=*) PREFIX="${arg#--prefix=}" ;;
    --no-modify-path) NO_MODIFY_PATH=1 ;;
    --force) FORCE=1 ;;
    --from-source) FROM_SOURCE=1 ;;
    --*) echo "unknown option: $arg" >&2; exit 1 ;;
  esac
  shift 2>/dev/null || true
done

# Allow --version as first arg without flag
if [ "${1:-}" != "" ] && [ "${VERSION}" = "latest" ]; then
  case "$1" in v*|0.*) VERSION="$1" ;; esac
fi

# Detect OS and arch
detect_platform() {
  OS="$(uname -s)"
  ARCH="$(uname -m)"
  case "$OS" in
    Linux) OS="unknown-linux-musl" ;;
    Darwin) OS="apple-darwin" ;;
    *) echo "Unsupported OS: $OS (only Linux/macOS via this script; use install.ps1 on Windows)" >&2; exit 1 ;;
  esac
  case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    arm64|aarch64) ARCH="aarch64" ;;
    *) echo "Unsupported arch: $ARCH" >&2; exit 1 ;;
  esac
  # Special case: macOS uses gnu? No, apple-darwin covers both
  if [ "$OS" = "apple-darwin" ]; then
    TARGET="${ARCH}-apple-darwin"
  else
    TARGET="${ARCH}-unknown-linux-musl"
  fi
  echo "$TARGET"
}

TARGET="$(detect_platform)"
echo "Detected target: $TARGET"

# Determine install prefix
if [ -z "$PREFIX" ]; then
  if [ -d "$HOME/.local/bin" ] || mkdir -p "$HOME/.local/bin" 2>/dev/null; then
    PREFIX="$HOME/.local/bin"
  elif [ -w "/usr/local/bin" ]; then
    PREFIX="/usr/local/bin"
  else
    PREFIX="$HOME/.local/bin"
    mkdir -p "$PREFIX"
  fi
fi
echo "Install prefix: $PREFIX"
mkdir -p "$PREFIX"

# Check for existing binary
if [ -f "$PREFIX/txtify" ] && [ "${FORCE:-}" != "1" ]; then
  echo "Existing txtify found at $PREFIX/txtify (use --force to overwrite)" >&2
fi

# Resolve version -> download URL
if [ "${FROM_SOURCE:-}" = "1" ]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found; install Rust via https://rustup.rs" >&2; exit 1
  fi
  echo "Building from source via cargo..."
  cargo install txtify --locked --all-features --root "$PREFIX/.."
  "$PREFIX/txtify" --version || "$HOME/.cargo/bin/txtify" --version || true
  echo "Installed via cargo to $PREFIX/txtify"
  exit 0
fi

resolve_url() {
  VER="$1"
  TGT="$2"
  if [ "$VER" = "latest" ]; then
    echo "https://github.com/${REPO}/releases/latest/download/txtify-${TGT}.tar.gz"
  else
    # Ensure v prefix
    case "$VER" in v*) ;; *) VER="v$VER" ;; esac
    echo "https://github.com/${REPO}/releases/download/${VER}/txtify-${TGT}.tar.gz"
  fi
}

URL="$(resolve_url "$VERSION" "$TARGET")"
SHA_URL="${URL}.sha256"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT INT TERM

echo "Downloading $URL..."
if command -v curl >/dev/null 2>&1; then
  if ! curl -fsSL --retry 3 --retry-delay 2 -o "$TMPDIR/txtify.tar.gz" "$URL"; then
    echo "Download failed: $URL" >&2
    echo "Tip: check https://github.com/${REPO}/releases" >&2
    exit 1
  fi
  # Try checksum if available
  if curl -fsSL -o "$TMPDIR/txtify.tar.gz.sha256" "$SHA_URL" 2>/dev/null; then
    echo "Verifying checksum..."
    # SHA file format: "<hash>  txtify-*.tar.gz" or just hash
    EXPECTED="$(awk '{print $1}' "$TMPDIR/txtify.tar.gz.sha256")"
    if command -v sha256sum >/dev/null 2>&1; then
      ACTUAL="$(sha256sum "$TMPDIR/txtify.tar.gz" | awk '{print $1}')"
    elif command -v shasum >/dev/null 2>&1; then
      ACTUAL="$(shasum -a 256 "$TMPDIR/txtify.tar.gz" | awk '{print $1}')"
    else
      echo "Warning: no sha256sum/shasum found, skipping verification" >&2
      ACTUAL="$EXPECTED"
    fi
    if [ "$ACTUAL" != "$EXPECTED" ]; then
      echo "Checksum mismatch! expected $EXPECTED got $ACTUAL" >&2
      exit 1
    fi
    echo "Checksum OK"
  else
    echo "No checksum file at $SHA_URL (skipping verification)"
  fi
elif command -v wget >/dev/null 2>&1; then
  if ! wget -qO "$TMPDIR/txtify.tar.gz" "$URL"; then
    echo "Download failed: $URL" >&2; exit 1
  fi
else
  echo "curl or wget required" >&2; exit 1
fi

echo "Extracting..."
if ! tar -xzf "$TMPDIR/txtify.tar.gz" -C "$TMPDIR"; then
  echo "Failed to extract tarball" >&2; exit 1
fi

# Find binary (tar may contain txtify or txtify-<target>/txtify)
BIN_SRC="$(find "$TMPDIR" -type f -name "txtify" | head -n 1)"
if [ -z "$BIN_SRC" ]; then
  echo "txtify binary not found in archive" >&2; ls -R "$TMPDIR" >&2; exit 1
fi

# Install
if [ -w "$PREFIX" ]; then
  install -m 755 "$BIN_SRC" "$PREFIX/txtify"
else
  echo "Need sudo to write to $PREFIX" >&2
  sudo install -m 755 "$BIN_SRC" "$PREFIX/txtify"
fi

echo "Installed txtify to $PREFIX/txtify"
"$PREFIX/txtify" --version || true

# Add to PATH if needed
if [ -z "$NO_MODIFY_PATH" ]; then
  case ":$PATH:" in
    *":$PREFIX:"*) echo "PATH already contains $PREFIX" ;;
    *)
      echo "Adding $PREFIX to PATH via shell rc..."
      SHELL_RC=""
      # Detect shell
      if [ -n "${ZSH_VERSION:-}" ] || [ "${SHELL:-}" = "/bin/zsh" ] || [ -f "$HOME/.zshrc" ]; then
        SHELL_RC="$HOME/.zshrc"
      elif [ -f "$HOME/.bashrc" ]; then
        SHELL_RC="$HOME/.bashrc"
      elif [ -f "$HOME/.bash_profile" ]; then
        SHELL_RC="$HOME/.bash_profile"
      fi
      if [ -n "$SHELL_RC" ] && [ -f "$SHELL_RC" ]; then
        if ! grep -q "$PREFIX" "$SHELL_RC" 2>/dev/null; then
          printf '\n# txtify\nexport PATH="%s:$PATH"\n' "$PREFIX" >> "$SHELL_RC"
          echo "Added to $SHELL_RC (restart shell or run: export PATH=\"$PREFIX:\$PATH\")"
        fi
      else
        echo "Add manually: export PATH=\"$PREFIX:\$PATH\""
      fi
      ;;
  esac
fi

echo ""
echo "Run 'txtify --help' and 'txtify doctor' to verify."
echo "To add shell integration: txtify shell install"
# shellcheck disable=SC2016
echo 'To use without restart: export PATH="'"$PREFIX"':$PATH"'
