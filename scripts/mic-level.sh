#!/usr/bin/env bash
#
# mic-level.sh — realtime microphone level meter for ghostty-voice.
#
# Run it and watch the bar while you talk: it shows your live input level (RMS +
# peak, as % of full scale and dBFS) against the silence threshold the VAD /
# streaming modes actually use, so you can SEE when you're above or below it
# instead of guessing. VAD / streaming / continuous capture via `sox -d` (the
# default input) and trim everything below `threshold_pct`; `toggle` uses
# `pw-record` and has no threshold. If `toggle` transcribes but the others say
# "no speech", your speech is below the line this meter draws — raise mic gain or
# lower the threshold.
#
# Usage:
#   scripts/mic-level.sh                 # meter the default input (what sox -d uses)
#   scripts/mic-level.sh --threshold 0.5 # override the threshold marker (% full scale)
#   scripts/mic-level.sh --source NAME   # meter a specific PipeWire source
#   scripts/mic-level.sh --list          # list capture sources, then exit
#
# Deps: sox (default capture) or pw-record (--source), and python3. Ctrl-C stops
# and prints the session peak.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RATE=16000
SOURCE=""
THRESH_OVERRIDE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source) SOURCE="${2:-}"; shift 2 ;;
    --threshold) THRESH_OVERRIDE="${2:-}"; shift 2 ;;
    --list)
      echo "Capture sources (name to pass to --source):"
      pactl list sources short 2>/dev/null | awk '$2 !~ /\.monitor$/ {print "  " $2}'
      echo
      echo "Default source: $(pactl get-default-source 2>/dev/null || echo '?')"
      exit 0 ;;
    -h|--help)
      sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

# Pull the configured thresholds so the meter line matches what the daemon does.
CONF="${XDG_CONFIG_HOME:-$HOME/.config}/ghostty-voice/config.toml"
read_pct() { # $1 = key name; prints its value or empty
  [[ -f "$CONF" ]] || return 0
  grep -E "^\s*$1\s*=" "$CONF" | head -1 | sed 's/.*=//; s/#.*//; s/[[:space:]]//g'
}
THR_VAD="$(read_pct vad_threshold_pct)";        THR_VAD="${THR_VAD:-3}"
THR_STREAM="$(read_pct silence_threshold_pct)"; THR_STREAM="${THR_STREAM:-$THR_VAD}"
THRESH="${THRESH_OVERRIDE:-$THR_VAD}"

# Capture command: `sox -d` (the exact path VAD/streaming use) by default, or
# pw-record targeting a chosen source. Both emit s16le mono; the meter handles a
# WAV header (pw-record) or raw stream (sox -t raw) transparently.
if [[ -n "$SOURCE" ]]; then
  capture=(pw-record --target="$SOURCE" --rate="$RATE" --channels=1 --format=s16 -)
  src_label="$SOURCE (pw-record)"
else
  capture=(sox -q -d -t raw -r "$RATE" -c 1 -b 16 -e signed-integer -)
  src_label="default input via 'sox -d' (the VAD/streaming capture path)"
fi

echo "ghostty-voice mic level meter"
echo "  source:    $src_label"
echo "  threshold: ${THRESH}% full scale  (VAD=${THR_VAD}%, streaming=${THR_STREAM}%)"
echo "  The bar is your SUSTAINED level (RMS) — that's what sox's silence detector"
echo "  uses, NOT peaks. Talk: the bar should cross the ┃ marker (turn green, ▶ SOUND)"
echo "  while you speak and drop below it when silent. If only brief peaks cross but"
echo "  the bar stays under the marker, VAD treats your speech as silence — lower the"
echo "  threshold below your speaking bar.  Ctrl-C to stop."
echo

# The audio flows on the pipe into the meter's stdin; the program itself is a
# separate file, so the two never collide.
"${capture[@]}" 2>/dev/null | python3 "$SCRIPT_DIR/mic-level.py" "$THRESH" "$RATE"
