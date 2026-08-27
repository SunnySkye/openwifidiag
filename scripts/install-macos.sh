#!/usr/bin/env sh
set -eu

# Local-only macOS installer. This script never downloads release assets or
# source files; run it from an openwifidiag checkout.

PREFIX="${OPENWIFIDIAG_PREFIX:-/usr/local}"
APP_ROOT="${OPENWIFIDIAG_APP_ROOT:-$HOME/Applications}"
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

case "$(uname -s)" in
  Darwin) ;;
  *) fail "This installer supports macOS only." ;;
esac

case "$(uname -m)" in
  arm64|x86_64) ;;
  *) fail "Unsupported macOS architecture: $(uname -m)" ;;
esac

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" 2>/dev/null && pwd) || \
  fail "Could not locate the installer directory."
source_dir=$(dirname "$script_dir")
plist="$source_dir/resources/macos/Info.plist"
entitlements="$source_dir/resources/macos/entitlements.plist"

printf '\n%s%s' "$BOLD" "$AQUA"
printf '  ╭────────────────────────────────────────────────────╮\n'
printf '  │                                                    │\n'
printf '  │   OpenWiFiDiag  ·  macOS local installer          │\n'
printf '  │   Signal clarity, beautifully packaged.           │\n'
printf '  │                                                    │\n'
printf '  ╰────────────────────────────────────────────────────╯\n'
printf '%s\n' "$RESET"

progress 5 "Checking local project"
[ -f "$source_dir/Cargo.toml" ] || fail "Cargo.toml was not found at $source_dir. Run this script from the repository checkout."
[ -f "$plist" ] || fail "Missing local app metadata: $plist"
[ -f "$entitlements" ] || fail "Missing local entitlements: $entitlements"

tmp=$(mktemp -d "${TMPDIR:-/tmp}/openwifidiag-install.XXXXXX") || fail "Could not create a temporary directory."
cleanup() {
  rm -rf "$tmp"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

progress 15 "Preparing local files"
binary="$tmp/openwifidiag"
build_log="$tmp/build.log"

if [ -n "${OPENWIFIDIAG_BINARY:-}" ]; then
  local_binary=$OPENWIFIDIAG_BINARY
  case "$local_binary" in
    /*) ;;
    *) local_binary="$(pwd)/$local_binary" ;;
  esac
  [ -f "$local_binary" ] || fail "OPENWIFIDIAG_BINARY does not exist: $local_binary"
  progress 55 "Using supplied local binary"
  cp "$local_binary" "$binary" || fail "Could not stage $local_binary."
elif command -v cargo >/dev/null 2>&1; then
  progress 25 "Building optimized local binary"
  if (cd "$source_dir" && cargo build --release --locked --offline) >"$build_log" 2>&1; then
    progress 55 "Local build complete"
    cp "$source_dir/target/release/openwifidiag" "$binary" || fail "The local build did not produce a binary."
  else
    finish_progress
    printf '\n%sBuild output:%s\n' "$MUTED" "$RESET" >&2
    tail -n 20 "$build_log" >&2
    fail "The local Rust build failed."
  fi
elif [ -x "$HOME/.cargo/bin/cargo" ]; then
  progress 25 "Building optimized local binary"
  if (cd "$source_dir" && "$HOME/.cargo/bin/cargo" build --release --locked --offline) >"$build_log" 2>&1; then
    progress 55 "Local build complete"
    cp "$source_dir/target/release/openwifidiag" "$binary" || fail "The local build did not produce a binary."
  else
    finish_progress
    printf '\n%sBuild output:%s\n' "$MUTED" "$RESET" >&2
    tail -n 20 "$build_log" >&2
    fail "The local Rust build failed."
  fi
elif [ -x "$source_dir/target/release/openwifidiag" ]; then
  progress 55 "Using existing local release binary"
  cp "$source_dir/target/release/openwifidiag" "$binary" || fail "Could not stage the existing local binary."
else
  fail "No local binary or Rust toolchain was found. Build with 'cargo build --release --offline', or set OPENWIFIDIAG_BINARY to a local binary."
fi

chmod 755 "$binary"
app="$APP_ROOT/OpenWiFiDiag.app"
staged_app="$tmp/OpenWiFiDiag.app"
dest_dir="$PREFIX/bin"
dest="$dest_dir/openwifidiag"

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
    sudo mkdir -p "$dest_dir" || fail "Could not create $dest_dir"
  fi
fi
if [ -w "$dest_dir" ]; then
  ln -sfn "$app/Contents/MacOS/openwifidiag" "$dest" || fail "Could not link the terminal command."
else
  sudo ln -sfn "$app/Contents/MacOS/openwifidiag" "$dest" || fail "Could not link the terminal command."
fi

progress 100 "Installation complete"
finish_progress

printf '\n  %s%s✓ OpenWiFiDiag is ready%s\n\n' "$GREEN" "$BOLD" "$RESET"
info "App       $app"
info "Command   $dest"
info "Launch    openwifidiag"
printf '\n  %sNo files were downloaded during installation.%s\n\n' "$MUTED" "$RESET"
