#!/usr/bin/env bash
set -euo pipefail

echo "=================================================="
echo "      TITANDAW AUTOMATED PRODUCTION BUILD         "
echo "=================================================="

# 1. Automated Code Patching Layer (Fixes both C and Rust types)
echo "Staging source verification..."
python3 -c '
import re
import os

# --- PATCH PART A: Fix ALSA triple-pointer signature in src/audio_engine.c ---
c_path = "src/audio_engine.c"
if os.path.exists(c_path):
    with open(c_path, "r") as f:
        c_content = f.read()

    fixed_c_function = """int titan_list_devices(char out[][64], int max) {
    void **hints;
    
    if (snd_device_name_hint(-1, "pcm", &hints) < 0) {
        return 0;
    }
    
    int count = 0;
    void **p = hints;
    
    while (*p && count < max) {
        char *name = snd_device_name_get_hint(*p, "NAME");
        if (name) {
            strncpy(out[count], name, 63);
            out[count][63] = 0;
            free(name);
            count++;
        }
        p++;
    }
    
    snd_device_name_free_hint(hints);
    return count;
}"""

    start_match = re.search(r"int\s+titan_list_devices\s*\(", c_content)
    if start_match:
        start_idx = start_match.start()
        brace_count = 0
        end_idx = -1
        for i in range(start_idx, len(c_content)):
            if c_content[i] == "{":
                brace_count += 1
            elif c_content[i] == "}":
                brace_count -= 1
                if brace_count == 0:
                    end_idx = i + 1
                    break
        if end_idx != -1:
            c_content = c_content[:start_idx] + fixed_c_function + c_content[end_idx:]
            with open(c_path, "w") as f:
                f.write(c_content)
            print("✓ src/audio_engine.c patched successfully (ALSA void*** alignment fix).")

# --- PATCH PART B: Fix String Type Slice Mismatch in src/ui.rs ---
ui_path = "src/ui.rs"
if os.path.exists(ui_path):
    with open(ui_path, "r") as f:
        ui_content = f.read()
    
    # Strip out the problematic byte-string specifier character 'b' before the CSS block
    if "b\"window {" in ui_content:
        ui_content = ui_content.replace("b\"window {", "\"window {")
        with open(ui_path, "w") as f:
            f.write(ui_content)
        print("✓ src/ui.rs patched successfully (Removed byte-string b literal constraint).")
'

# 2. Clean Compilation Layer
echo ""
echo "=== Purging Old Artifact Caches & Rebuilding ==="
cargo clean
cargo build --release

# 3. Dynamic Environment Transport Layer
echo ""
echo "=== Pre-Flight Check & Launch ==="
[ -f device.conf ] && source device.conf || true
DEVICE=${TITAN_ALSA_DEVICE:-hw:1,0}

if [ -f "titan_producer_brain.py" ]; then
  source venv/bin/activate 2>/dev/null || true
  echo "Booting AI Production Brain Subservice..."
  python titan_producer_brain.py &
  BRAIN_PID=$!
  sleep 2
fi

echo "Deploying host window on audio interface address: $DEVICE"
TITAN_ALSA_DEVICE="$DEVICE" ./target/release/titandaw

on_exit() {
  if [ -n "${BRAIN_PID:-}" ]; then
    kill "$BRAIN_PID" 2>/dev/null || true
  fi
}
trap on_exit EXIT
