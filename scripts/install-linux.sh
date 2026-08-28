#!/usr/bin/env sh
set -eu

REPO="SunnySkye/openwifidiag"
VERSION="${OPENWIFIDIAG_VERSION:-latest}"
PREFIX="${OPENWIFIDIAG_PREFIX:-/usr/local}"

usage() {
  printf 'Usage: %s [options]\n\n' "$0"
  printf 'Options:\n'
  printf '  --binary PATH   Install this local openwifidiag binary instead of downloading a release.\n'
  printf '  --source        Build and install from the local source checkout (requires cargo).\n'
  printf '  -h, --help      Show this help and exit.\n\n'
  printf 'Environment:\n'
  printf '  OPENWIFIDIAG_BINARY   Same as --binary.\n'
  printf '  OPENWIFIDIAG_VERSION  Release tag to download (default: latest).\n'
  printf '  OPENWIFIDIAG_PREFIX   Install prefix (default: /usr/local).\n'
}

# Flags win over environment variables; without either a release is downloaded.
binary_override="${OPENWIFIDIAG_BINARY:-}"
from_source="${OPENWIFIDIAG_BUILD_FROM_SOURCE:-0}"
while [ $# -gt 0 ]; do
  case "$1" in
    --binary)
      [ $# -ge 2 ] || { echo "The --binary option requires a path argument." >&2; exit 1; }
      binary_override=$2
      shift 2
      ;;
    --binary=*)
      binary_override=${1#*=}
      shift
      ;;
    --source)
      from_source=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
done

case "$(uname -m)" in
  x86_64|amd64) platform="linux-x64" ;;
  aarch64|arm64) platform="linux-arm64" ;;
  *) echo "Unsupported Linux architecture: $(uname -m)" >&2; exit 1 ;;
esac

case "$(uname -s)" in
  Linux) ;;
  *) echo "This installer supports Linux only." >&2; exit 1 ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

if [ -n "$binary_override" ]; then
  case "$binary_override" in
    /*) ;;
    *) binary_override="$(pwd)/$binary_override" ;;
  esac
  [ -f "$binary_override" ] || { echo "--binary does not exist: $binary_override" >&2; exit 1; }
  cp "$binary_override" "$tmp/openwifidiag"
  source_message="Installed from the supplied local binary."
elif [ "$from_source" = "1" ]; then
  source_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." 2>/dev/null && pwd)
  [ -f "$source_dir/Cargo.toml" ] || { echo "--source requires a source checkout next to this installer." >&2; exit 1; }
  if command -v cargo >/dev/null 2>&1; then
    cargo_cmd=$(command -v cargo)
  elif [ -x "$HOME/.cargo/bin/cargo" ]; then
    cargo_cmd="$HOME/.cargo/bin/cargo"
  else
    echo "--source requires cargo; install Rust from https://rustup.rs and retry." >&2
    exit 1
  fi
  echo "Building release binary from $source_dir …"
  (cd "$source_dir" && "$cargo_cmd" build --release --locked) || {
    echo "The local Rust build failed." >&2
    exit 1
  }
  cp "$source_dir/target/release/openwifidiag" "$tmp/openwifidiag"
  source_message="Built and installed from the local source checkout."
else
  if [ "$VERSION" = latest ]; then
    url="https://github.com/$REPO/releases/latest/download/openwifidiag-$platform"
  else
    url="https://github.com/$REPO/releases/download/$VERSION/openwifidiag-$platform"
  fi
  curl -fL "$url" -o "$tmp/openwifidiag"
  source_message="Installed from a prebuilt GitHub Release."
fi
chmod 755 "$tmp/openwifidiag"

dest="$PREFIX/bin/openwifidiag"
if [ -w "$(dirname "$dest")" ]; then
  install -m 755 "$tmp/openwifidiag" "$dest"
else
  sudo install -m 755 "$tmp/openwifidiag" "$dest"
fi
printf 'Installed %s to %s\n' "$("$dest" --version 2>/dev/null || printf 'openwifidiag')" "$dest"
printf '%s\n' "$source_message"
