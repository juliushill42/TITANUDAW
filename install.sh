#!/usr/bin/env bash
# TitanDAW — Install + Build   JCH-2026-011
# Run once on EndeavourOS / Arch Linux
set -euo pipefail

echo "=== TitanDAW Install ==="

# ── System deps ──────────────────────────────────────────────────────────────
sudo pacman -S --needed --noconfirm \
  rust cargo \
  gtk4 gtk4-devel \
  alsa-lib alsa-utils \
  alsa-plugins \
  pkg-config \
  gcc \
  python \
  python-pip \
  python-virtualenv

# ── Rust toolchain ────────────────────────────────────────────────────────────
rustup default stable
rustup update

# ── Detect AudioBox USB 96 ────────────────────────────────────────────────────
echo ""
echo "=== ALSA Devices ==="
aplay -l
echo ""

# Find AudioBox hw address
DEVICE=$(aplay -l | grep -i "audiobox\|usb audio\|usb-audio" | head -1 | \
         sed -E 's/card ([0-9]+): .* device ([0-9]+):.*/hw:\1,\2/')

if [ -z "$DEVICE" ]; then
  echo "⚠ AudioBox not detected — defaulting to hw:1,0"
  echo "  Make sure USB cable is connected and run: aplay -l"
  DEVICE="hw:1,0"
else
  echo "✓ AudioBox detected: $DEVICE"
fi

# Write device config
cat > device.conf <<EOF
TITAN_ALSA_DEVICE=$DEVICE
TITAN_SAMPLE_RATE=48000
TITAN_BRAIN_URL=http://127.0.0.1:5002
TITAN_MIDI_PORT=AudioBox
EOF
echo "✓ device.conf written"

# ── Python brain deps ─────────────────────────────────────────────────────────
if [ ! -d "venv" ]; then
  python -m venv venv
fi
source venv/bin/activate
pip install --upgrade pip
pip install \
  flask flask-cors \
  torch --index-url https://download.pytorch.org/whl/cpu \
  transformers peft accelerate \
  basic-pitch \
  openai-whisper \
  sounddevice \
  mido python-rtmidi \
  scipy numpy \
  requests
deactivate
echo "✓ Python deps installed"

# ── Build Rust binary ─────────────────────────────────────────────────────────
echo ""
echo "=== Building TitanDAW ==="
ALSA_LIB_DIR=$(pkg-config --variable=libdir alsa) \
PKG_CONFIG_PATH=$(pkg-config --variable=libdir alsa)/pkgconfig:$PKG_CONFIG_PATH \
cargo build --release 2>&1

echo ""
echo "=== Build Complete ==="
echo "Binary: ./target/release/titandaw"
echo ""
echo "Run: ./run.sh"
