#!/usr/bin/env sh
set -eu

REPO="SunnySkye/openwifidiag"
VERSION="${OPENWIFIDIAG_VERSION:-latest}"
PREFIX="${OPENWIFIDIAG_PREFIX:-/usr/local}"

case "$(uname -m)" in
  x86_64|amd64) platform="linux-x64" ;;
  aarch64|arm64) platform="linux-arm64" ;;
  *) echo "Unsupported Linux architecture: $(uname -m)" >&2; exit 1 ;;
esac

if [ "$VERSION" = latest ]; then
  url="https://github.com/$REPO/releases/latest/download/openwifidiag-$platform"
else
  url="https://github.com/$REPO/releases/download/$VERSION/openwifidiag-$platform"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
curl -fL "$url" -o "$tmp/openwifidiag"
chmod 755 "$tmp/openwifidiag"

dest="$PREFIX/bin/openwifidiag"
if [ -w "$(dirname "$dest")" ]; then
  install -m 755 "$tmp/openwifidiag" "$dest"
else
  sudo install -m 755 "$tmp/openwifidiag" "$dest"
fi
echo "Installed openwifidiag to $dest"
