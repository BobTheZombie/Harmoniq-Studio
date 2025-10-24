#!/usr/bin/env bash
set -euo pipefail

MINIDUMP_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/HarmoniqStudio/minidumps"
mkdir -p "$MINIDUMP_DIR"
export HARMONIQ_MINIDUMP_DIR="$MINIDUMP_DIR"

# Ensure PipeWire/JACK bridges get priority when available
export PIPEWIRE_LATENCY="${PIPEWIRE_LATENCY:-128/48000}"
export RUST_LOG=${RUST_LOG:-warn}

exec /app/lib/harmoniq-studio/harmoniq-studio-bin "$@"
