#!/usr/bin/env sh
# txtify installer - curl | sh for Linux/macOS
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/jeremymiribung-blip/txtify/main/installer/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/jeremymiribung-blip/txtify/main/installer/install.sh | sh -s -- --help
#   curl -fsSL .../install.sh | sh -s -- --with-model --with-shell
#   curl -fsSL https://github.com/jeremymiribung-blip/txtify/releases/latest/download/install.sh -o install.sh && sh install.sh --prefix ~/.local/bin
# Security: always inspect script before piping to sh (less install.sh)
#
# Der Installer lädt automatisch alle Ressourcen nach:
#   Binary + `txtify setup` (Python-Deps, KI-Modell zai-org/GLM-OCR ~1GB, Config).
#   Mit --no-setup nur das Binary installieren (z.B. für CI / Docker).
#
# Environment variables:
#   TXTIFY_VERSION - version to install (default: latest)
#   TXTIFY_REPO - GitHub repo (default: jeremymiribung-blip/txtify)
#   TXTIFY_PREFIX - install directory (default: ~/.local/bin, fallback /usr/local/bin if writable)
#   TXTIFY_NO_MODIFY_PATH - if set, do not modify shell rc files
#   TXTIFY_SETUP - 1 (default) = `txtify setup` nach der Installation ausführen, 0 = überspringen
#   TXTIFY_NO_MODEL - if set, KI-Modell nicht herunterladen (~1GB sparen)
#   TXTIFY_WITH_SHELL - if set, auch `shell install` (Kontextmenü) ausführen
#   TXTIFY_MODEL - Modell-ID (default: zai-org/GLM-OCR)

set -eu

REPO="${TXTIFY_REPO:-jeremymiribung-blip/txtify}"
VERSION="${TXTIFY_VERSION:-latest}"
PREFIX="${TXTIFY_PREFIX:-}"
NO_MODIFY_PATH="${TXTIFY_NO_MODIFY_PATH:-}"
SETUP="${TXTIFY_SETUP:-1}"
NO_MODEL="${TXTIFY_NO_MODEL:-}"
WITH_SHELL="${TXTIFY_WITH_SHELL:-}"
MODEL="${TXTIFY_MODEL:-zai-org/GLM-OCR}"
GITHUB_RELEASES="https://github.com/${REPO}/releases"

# Parse args (POSIX while-loop, shift-sicher)
while [ "$#" -gt 0 ]; do
  arg="$1"
  case "$arg" in
    --help|-h)
      cat <<'EOF'
txtify installer (Linux/macOS) — inkl. Auto-Setup (KI-Modell, Python-Deps, Config)

Usage: sh install.sh [OPTIONS]

One-liner (empfohlen, lädt alles inkl. KI-Modell ~1GB):
  curl -fsSL https://raw.githubusercontent.com/jeremymiribung-blip/txtify/main/installer/install.sh | sh

Optionen:
  --help, -h          Diese Hilfe
  --version VERSION   Version (z.B. v0.1.0, default: latest)
  --prefix DIR        Installationsverzeichnis (default: ~/.local/bin)
  --no-modify-path    Shell-rc-Dateien nicht anfassen
  --force             Existierendes Binary überschreiben
  --from-source       Aus Source bauen (braucht Rust/cargo)
  --with-setup        `txtify setup` nach Installation ausführen (Default)
  --no-setup          Nur Binary installieren, kein Setup/Modell-Download
  --with-model        KI-Modell herunterladen (Default, ~1GB Hugging Face)
  --no-model          KI-Modell NICHT herunterladen (nur Fast-Modus)
  --with-shell        Auch Rechtsklick-Menü installieren (`shell install`)
  --model ID          Andere Modell-ID (default: zai-org/GLM-OCR)
  --yes               Nicht-interaktiv (für setup)

Environment:
  TXTIFY_VERSION, TXTIFY_REPO, TXTIFY_PREFIX, TXTIFY_NO_MODIFY_PATH,
  TXTIFY_SETUP=0/1, TXTIFY_NO_MODEL=1, TXTIFY_WITH_SHELL=1, TXTIFY_MODEL=...

Beispiele:
  curl -fsSL .../install.sh | sh                                    # alles inkl. KI-Modell
  curl -fsSL .../install.sh | sh -s -- --no-model                   # nur Fast-Modus, schnell
  TXTIFY_NO_MODEL=1 sh install.sh                                   # dto. per Env
  sh install.sh --prefix ~/.cargo/bin --with-shell
EOF
      exit 0
      ;;
    --version) VERSION="$2"; shift 2 ;;
    --version=*) VERSION="${arg#--version=}"; shift ;;
    --prefix) PREFIX="$2"; shift 2 ;;
    --prefix=*) PREFIX="${arg#--prefix=}"; shift ;;
    --no-modify-path) NO_MODIFY_PATH=1; shift ;;
    --force) FORCE=1; shift ;;
    --from-source) FROM_SOURCE=1; shift ;;
    --with-setup) SETUP=1; shift ;;
    --no-setup) SETUP=0; shift ;;
    --with-model) NO_MODEL=""; shift ;;
    --no-model) NO_MODEL=1; shift ;;
    --with-shell) WITH_SHELL=1; shift ;;
    --model) MODEL="$2"; shift 2 ;;
    --model=*) MODEL="${arg#--model=}"; shift ;;
    --yes|-y) YES=1; shift ;;
    --*) echo "unknown option: $arg (siehe --help)" >&2; exit 1 ;;
    *) # Positionsarg: v0.1.0 als Version tolerieren
      case "$arg" in v*|0.*) VERSION="$arg" ;; *) echo "unknown argument: $arg" >&2; exit 1 ;; esac
      shift ;;
  esac
done

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

# Install function shared by binary + from-source paths
do_post_install() {
  # Add to PATH if needed
  if [ -z "$NO_MODIFY_PATH" ]; then
    case ":$PATH:" in
      *":$PREFIX:"*) echo "PATH already contains $PREFIX" ;;
      *)
        echo "Adding $PREFIX to PATH via shell rc..."
        SHELL_RC=""
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
  # Ensure binary is on PATH for setup step
  export PATH="$PREFIX:$PATH"

  if [ "$SETUP" = "0" ]; then
    echo ""
    echo "Setup übersprungen (--no-setup)."
    echo "Später nachholen: txtify setup --yes   # lädt Python-Deps + KI-Modell (~1GB)"
    echo "Oder nur prüfen:  txtify doctor"
    return 0
  fi

  echo ""
  echo "Running auto-setup (Python-Deps + KI-Modell + Config)..."
  SETUP_ARGS="--yes"
  if [ -n "$NO_MODEL" ]; then
    SETUP_ARGS="$SETUP_ARGS --no-model"
  else
    SETUP_ARGS="$SETUP_ARGS --model $MODEL"
  fi
  if [ -n "$WITH_SHELL" ]; then
    SETUP_ARGS="$SETUP_ARGS --with-shell"
  fi
  # shellcheck disable=SC2086
  if "$PREFIX/txtify" setup $SETUP_ARGS; then
    echo "Auto-setup erfolgreich."
  else
    echo "WARN: 'txtify setup' meldete Fehler — Binary funktioniert trotzdem im Fast-Modus." >&2
    echo "Manuell erneut versuchen: txtify setup --yes" >&2
    echo "Details: txtify doctor" >&2
  fi
}

# Resolve version -> download URL
if [ "${FROM_SOURCE:-}" = "1" ]; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found; install Rust via https://rustup.rs" >&2; exit 1
  fi
  echo "Building from source via cargo..."
  cargo install txtify --locked --all-features --root "$PREFIX/.."
  "$PREFIX/txtify" --version || "$HOME/.cargo/bin/txtify" --version || true
  echo "Installed via cargo to $PREFIX/txtify"
  do_post_install
  echo ""
  echo "Run 'txtify --help' and 'txtify doctor' to verify."
  # shellcheck disable=SC2016
  echo 'To use without restart: export PATH="'"$PREFIX"':$PATH"'
  exit 0
fi

resolve_url() {
  VER="$1"
  TGT="$2"
  if [ "$VER" = "latest" ]; then
    echo "https://github.com/${REPO}/releases/latest/download/txtify-${TGT}.tar.gz"
  else
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
  if curl -fsSL -o "$TMPDIR/txtify.tar.gz.sha256" "$SHA_URL" 2>/dev/null; then
    echo "Verifying checksum..."
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

do_post_install

echo ""
echo "Run 'txtify --help' and 'txtify doctor' to verify."
echo "Fast-Modus geht sofort; High-Quality nutzt das geladene GLM-OCR-Modell."
# shellcheck disable=SC2016
echo 'To use without restart: export PATH="'"$PREFIX"':$PATH"'
