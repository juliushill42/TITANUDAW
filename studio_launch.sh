#!/usr/bin/env bash
set -euo pipefail
cd /home/juliush/Documents/titandaw
[ -f device.conf ] && source device.conf || true
DEVICE=${TITAN_ALSA_DEVICE:-hw:1,0}

if [ -f "titan_producer_brain.py" ]; then
  source venv/bin/activate 2>/dev/null || true
  echo "Booting AI Production Brain Subservice..."
  python titan_producer_brain.py &
  BRAIN_PID=$!
  sleep 2
fi

echo "Deploying Workspace Windows on interface address: $DEVICE"
TITAN_ALSA_DEVICE="$DEVICE" ./target/release/titandaw

on_exit() {
  if [ -n "${BRAIN_PID:-}" ]; then
    kill "$BRAIN_PID" 2>/dev/null || true
  fi
}
trap on_exit EXIT
