#!/usr/bin/env sh
set -eu

# Standalone macOS installer. It downloads a prebuilt GitHub Release by default
# and can optionally build from a source checkout for development.

PREFIX="${OPENWIFIDIAG_PREFIX:-/usr/local}"
APP_ROOT="${OPENWIFIDIAG_APP_ROOT:-$HOME/Applications}"
REPO="SunnySkye/openwifidiag"
VERSION="${OPENWIFIDIAG_VERSION:-latest}"
BAR_WIDTH=28

if [ -t 1 ] && [ "${TERM:-dumb}" != "dumb" ] && [ -z "${NO_COLOR:-}" ]; then
  RESET=$(printf '\033[0m')
  BOLD=$(printf '\033[1m')
  AQUA=$(printf '\033[38;5;81m')
  VIOLET=$(printf '\033[38;5;141m')
  GREEN=$(printf '\033[38;5;114m')
  CORAL=$(printf '\033[38;5;203m')
  MUTED=$(printf '\033[38;5;245m')
  CLEAR_LINE=$(printf '\033[2K')
  INTERACTIVE=1
else
  RESET=""
  BOLD=""
  AQUA=""
  VIOLET=""
  GREEN=""
  CORAL=""
  MUTED=""
  CLEAR_LINE=""
  INTERACTIVE=0
fi

progress() {
  percentage=$1
  label=$2
  filled=$((percentage * BAR_WIDTH / 100))
  empty=$((BAR_WIDTH - filled))
  completed_bar=""
  remaining_bar=""
  index=0
  while [ "$index" -lt "$filled" ]; do
    completed_bar="${completed_bar}━"
    index=$((index + 1))
  done
  index=0
  while [ "$index" -lt "$empty" ]; do
    remaining_bar="${remaining_bar}─"
    index=$((index + 1))
  done

  if [ "$INTERACTIVE" -eq 1 ]; then
    printf '\r%s  %s%s%s%s%s%s %s%s%3d%%%s  %s' \
      "$CLEAR_LINE" "$AQUA" "$completed_bar" "$RESET" "$MUTED" "$remaining_bar" "$RESET" \
      "$VIOLET" "$BOLD" "$percentage" "$RESET" "$label"
    printf '%s' "$RESET"
  else
    printf '[%3d%%] %s\n' "$percentage" "$label"
  fi
}

finish_progress() {
  if [ "$INTERACTIVE" -eq 1 ]; then
    printf '\n'
  fi
}

info() {
  printf '  %s◆%s %s\n' "$VIOLET" "$RESET" "$1"
}

fail() {
  finish_progress
  printf '\n  %s%sInstallation failed%s\n  %s\n' "$CORAL" "$BOLD" "$RESET" "$1" >&2
  exit 1
}

usage() {
  printf 'Usage: %s [options]\n\n' "$0"
  printf 'Options:\n'
  printf '  --binary PATH   Install this local openwifidiag binary instead of downloading a release.\n'
  printf '  --source        Build and install from the local source checkout (bootstraps Rust if needed).\n'
  printf '  -h, --help      Show this help and exit.\n\n'
  printf 'Environment:\n'
  printf '  OPENWIFIDIAG_BINARY              Same as --binary.\n'
  printf '  OPENWIFIDIAG_BUILD_FROM_SOURCE=1 Same as --source.\n'
  printf '  OPENWIFIDIAG_PREFIX              Install prefix for the command link (default: /usr/local).\n'
  printf '  OPENWIFIDIAG_VERSION             Release tag to download (default: latest).\n'
}

# Flags win over environment variables; without either a release is downloaded.
binary_override="${OPENWIFIDIAG_BINARY:-}"
from_source="${OPENWIFIDIAG_BUILD_FROM_SOURCE:-0}"
while [ $# -gt 0 ]; do
  case "$1" in
    --binary)
      [ $# -ge 2 ] || fail "The --binary option requires a path argument."
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
      fail "Unknown option: $1"
      ;;
  esac
done

case "$(uname -s)" in
  Darwin) ;;
  *) fail "This installer supports macOS only." ;;
esac

case "$(uname -m)" in
  arm64) platform="darwin-arm64" ;;
  x86_64) platform="darwin-x64" ;;
  *) fail "Unsupported macOS architecture: $(uname -m)" ;;
esac

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" 2>/dev/null && pwd) || \
  fail "Could not locate the installer directory."
source_dir=$(dirname "$script_dir")

printf '\n%s%s' "$BOLD" "$AQUA"
printf '  ╭────────────────────────────────────────────────────╮\n'
printf '  │                                                    │\n'
printf '  │   OpenWiFiDiag  ·  macOS installer                │\n'
printf '  │   Signal clarity, beautifully packaged.           │\n'
printf '  │                                                    │\n'
printf '  ╰────────────────────────────────────────────────────╯\n'
printf '%s\n' "$RESET"

progress 5 "Checking local project"
has_checkout=0
if [ -f "$source_dir/Cargo.toml" ]; then
  has_checkout=1
fi

tmp=$(mktemp -d "${TMPDIR:-/tmp}/openwifidiag-install.XXXXXX") || fail "Could not create a temporary directory."
cleanup() {
  rm -rf "$tmp"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

plist="$tmp/Info.plist"
entitlements="$tmp/entitlements.plist"
if [ -f "$source_dir/resources/macos/Info.plist" ] && [ -f "$source_dir/resources/macos/entitlements.plist" ]; then
  cp "$source_dir/resources/macos/Info.plist" "$plist"
  cp "$source_dir/resources/macos/entitlements.plist" "$entitlements"
else
  printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>' \
    '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
    '<plist version="1.0"><dict>' \
    '<key>CFBundleIdentifier</key><string>dev.openwifidiag.cli</string>' \
    '<key>CFBundleName</key><string>OpenWiFiDiag</string>' \
    '<key>CFBundleDisplayName</key><string>OpenWiFiDiag</string>' \
    '<key>CFBundleExecutable</key><string>openwifidiag</string>' \
    '<key>CFBundlePackageType</key><string>APPL</string>' \
    '<key>LSUIElement</key><true/>' \
    '<key>NSLocationWhenInUseUsageDescription</key><string>openwifidiag uses your location permission only to read nearby Wi-Fi network names and identifiers.</string>' \
    '<key>NSLocationUsageDescription</key><string>openwifidiag uses your location permission only to read nearby Wi-Fi network names and identifiers.</string>' \
    '</dict></plist>' >"$plist"
  printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>' \
    '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
    '<plist version="1.0"><dict>' \
    '<key>com.apple.security.personal-information.location</key><true/>' \
    '</dict></plist>' >"$entitlements"
fi

progress 15 "Preparing local files"
binary="$tmp/openwifidiag"
build_log="$tmp/build.log"
cargo_cmd=""

find_cargo() {
  if command -v cargo >/dev/null 2>&1; then
    cargo_cmd=$(command -v cargo)
  elif [ -x "$HOME/.cargo/bin/cargo" ]; then
    cargo_cmd="$HOME/.cargo/bin/cargo"
  fi
}

install_build_dependencies() {
  progress 18 "Checking build dependencies"
  if ! xcode-select -p >/dev/null 2>&1; then
    xcode-select --install >/dev/null 2>&1 || true
    fail "Apple Command Line Tools are required. Complete the installation window that just opened, then run this installer again."
  fi
  command -v curl >/dev/null 2>&1 || fail "curl is required to install the Rust toolchain."

  progress 22 "Installing the Rust toolchain"
  rustup_installer="$tmp/rustup-init.sh"
  curl --proto '=https' --tlsv1.2 -fsS https://sh.rustup.rs -o "$rustup_installer" || \
    fail "Could not download the official Rust installer from https://sh.rustup.rs."
  if ! sh "$rustup_installer" -y --profile minimal --no-modify-path >"$tmp/rustup.log" 2>&1; then
    tail -n 20 "$tmp/rustup.log" >&2
    fail "The Rust toolchain installation failed."
  fi
  find_cargo
  [ -n "$cargo_cmd" ] || fail "Rust was installed, but cargo could not be found at $HOME/.cargo/bin/cargo."
}

download_release_binary() {
  command -v curl >/dev/null 2>&1 || fail "curl is required to download the GitHub release."
  if [ "$VERSION" = "latest" ]; then
    url="https://github.com/$REPO/releases/latest/download/openwifidiag-$platform"
  else
    url="https://github.com/$REPO/releases/download/$VERSION/openwifidiag-$platform"
  fi
  progress 25 "Downloading $platform release"
  curl --proto '=https' --tlsv1.2 -fsSL "$url" -o "$binary" || \
    fail "Could not download $url. Check that the requested release and architecture exist."
  progress 55 "Release download complete"
}

build_from_source() {
  [ "$has_checkout" -eq 1 ] || fail "A source checkout is required for --source (or OPENWIFIDIAG_BUILD_FROM_SOURCE=1)."
  if [ -x "$source_dir/target/release/openwifidiag" ]; then
    progress 55 "Using existing local release binary"
    cp "$source_dir/target/release/openwifidiag" "$binary" || fail "Could not stage the existing local binary."
    return
  fi
  find_cargo
  if [ -z "$cargo_cmd" ]; then
    install_build_dependencies
  fi
  progress 25 "Building optimized local binary"
  if (cd "$source_dir" && "$cargo_cmd" build --release --locked) >"$build_log" 2>&1; then
    progress 55 "Local build complete"
    cp "$source_dir/target/release/openwifidiag" "$binary" || fail "The local build did not produce a binary."
  else
    finish_progress
    printf '\n%sBuild output:%s\n' "$MUTED" "$RESET" >&2
    tail -n 20 "$build_log" >&2
    fail "The local Rust build failed."
  fi
}

if [ -n "$binary_override" ]; then
  local_binary=$binary_override
  case "$local_binary" in
    /*) ;;
    *) local_binary="$(pwd)/$local_binary" ;;
  esac
  [ -f "$local_binary" ] || fail "--binary does not exist: $local_binary"
  progress 55 "Using supplied local binary"
  cp "$local_binary" "$binary" || fail "Could not stage $local_binary."
elif [ "$from_source" = "1" ]; then
  build_from_source
else
  download_release_binary
fi

chmod 755 "$binary"
app="$APP_ROOT/OpenWiFiDiag.app"
staged_app="$tmp/OpenWiFiDiag.app"
dest_dir="$PREFIX/bin"
dest="$dest_dir/openwifidiag"
sudo_ready=0

request_admin() {
  if [ "$sudo_ready" -eq 0 ]; then
    finish_progress
    printf '\n  %s%sAdministrator permission required%s\n' "$VIOLET" "$BOLD" "$RESET"
    printf '  macOS may ask for your password to install the terminal command in %s.\n\n' "$dest_dir"
    sudo -v || fail "Administrator permission was not granted."
    sudo_ready=1
  fi
}

progress 68 "Creating macOS app bundle"
mkdir -p "$staged_app/Contents/MacOS" || fail "Could not create the staged app bundle."
cp "$binary" "$staged_app/Contents/MacOS/openwifidiag" || fail "Could not copy the local binary into the app bundle."
cp "$plist" "$staged_app/Contents/Info.plist" || fail "Could not copy the local Info.plist."
chmod 755 "$staged_app/Contents/MacOS/openwifidiag"

progress 80 "Signing app bundle locally"
if ! codesign --force --deep --sign - --entitlements "$entitlements" "$staged_app" >"$tmp/codesign.log" 2>&1; then
  finish_progress
  tail -n 20 "$tmp/codesign.log" >&2
  fail "Local ad-hoc code signing failed."
fi

progress 90 "Installing application"
if ! mkdir -p "$APP_ROOT" 2>/dev/null; then
  fail "Could not create the application directory: $APP_ROOT"
fi
if ! ditto "$staged_app" "$app"; then
  fail "Could not install the app bundle to $app"
fi
if ! codesign --force --deep --sign - --entitlements "$entitlements" "$app" >"$tmp/codesign-installed.log" 2>&1; then
  fail "Could not sign the installed app bundle."
fi

progress 96 "Linking terminal command"
if [ ! -d "$dest_dir" ]; then
  if ! mkdir -p "$dest_dir" 2>/dev/null; then
    request_admin
    sudo mkdir -p "$dest_dir" || fail "Could not create $dest_dir"
    progress 96 "Linking terminal command"
  fi
fi
if [ -w "$dest_dir" ]; then
  ln -sfn "$app/Contents/MacOS/openwifidiag" "$dest" || fail "Could not link the terminal command."
else
  request_admin
  sudo ln -sfn "$app/Contents/MacOS/openwifidiag" "$dest" || fail "Could not link the terminal command."
  progress 96 "Linking terminal command"
fi

progress 100 "Installation complete"
finish_progress

printf '\n  %s%s✓ OpenWiFiDiag is ready%s\n\n' "$GREEN" "$BOLD" "$RESET"
info "App       $app"
info "Command   $dest"
info "Version   $("$dest" --version 2>/dev/null || printf 'unknown')"
info "Launch    openwifidiag"
if [ -n "$binary_override" ]; then
  printf '\n  %sInstalled from the supplied local binary.%s\n\n' "$MUTED" "$RESET"
elif [ "$from_source" = "1" ]; then
  printf '\n  %sBuilt and installed from the local source checkout.%s\n\n' "$MUTED" "$RESET"
else
  printf '\n  %sInstalled from a prebuilt GitHub Release; no Rust toolchain was required.%s\n\n' "$MUTED" "$RESET"
fi
