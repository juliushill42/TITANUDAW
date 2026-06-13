#!/usr/bin/env bash
# TitanDAW — Launch   JCH-2026-011
set -euo pipefail

[ -f device.conf ] && source device.conf || true
DEVICE=${TITAN_ALSA_DEVICE:-hw:1,0}
BRAIN_URL=${TITAN_BRAIN_URL:-http://127.0.0.1:5002}

echo "=== TitanDAW ==="
echo "ALSA: $DEVICE"

# Start SmolLM brain in background
if [ -f "titan_producer_brain.py" ]; then
  source venv/bin/activate 2>/dev/null || true
  echo "Starting AI Brain (port 5002)..."
  python titan_producer_brain.py &
  BRAIN_PID=$!
  sleep 2
  echo "Brain PID: $BRAIN_PID"
fi

# Launch DAW
TITAN_ALSA_DEVICE="$DEVICE" ./target/release/titandaw

# Cleanup on exit
[ -n "${BRAIN_PID:-}" ] && kill $BRAIN_PID 2>/dev/null || true
