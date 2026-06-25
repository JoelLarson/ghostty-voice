#!/usr/bin/env python3
"""Realtime input-level meter for ghostty-voice (driven by scripts/mic-level.sh).

Reads s16le mono PCM from stdin (raw from `sox -t raw`, or a WAV stream from
`pw-record` — the header is detected and skipped) and draws a live RMS bar with
the silence threshold marked, so you can see whether your speech clears it.

argv: <threshold_pct> <sample_rate>
"""

import array
import math
import sys

thresh_pct = float(sys.argv[1]) if len(sys.argv) > 1 else 3.0
rate = int(sys.argv[2]) if len(sys.argv) > 2 else 16000
stdin = sys.stdin.buffer

# Skip a WAV header if the capture emits one (pw-record); `sox -t raw` has none.
head = stdin.read(64)
if head[:4] == b"RIFF":
    i = head.find(b"data")
    pcm = head[i + 8:] if i != -1 else head[44:]
else:
    pcm = head

WIN = (rate // 10) * 2          # 100 ms windows (s16 = 2 bytes/sample)
BARW = 34
DB_FLOOR = -60.0
GREEN, DIM, BOLD, RESET = "\x1b[32m", "\x1b[2m", "\x1b[1m", "\x1b[0m"


def dbfs(x: float) -> float:
    return 20.0 * math.log10(x) if x > 1e-9 else DB_FLOOR


def db_pos(db: float) -> int:
    db = max(DB_FLOOR, min(0.0, db))
    return int(round((db - DB_FLOOR) / -DB_FLOOR * (BARW - 1)))


def bar(level_db: float, marker_db: float) -> str:
    fill = db_pos(level_db)
    mark = db_pos(marker_db)
    cells = []
    for i in range(BARW):
        ch = "█" if i <= fill else "░"  # full block / light shade
        if i == mark:
            ch = "╋" if ch == "█" else "┃"  # threshold line
        cells.append(ch)
    return "".join(cells)


thr_frac = thresh_pct / 100.0
thr_db = dbfs(thr_frac)
peak_hold = 0.0
sess_peak = 0.0
acc = pcm

sys.stdout.write("\x1b[?25l")  # hide cursor
try:
    while True:
        while len(acc) < WIN:
            chunk = stdin.read(WIN)
            if not chunk:
                raise EOFError
            acc += chunk
        frame, acc = acc[:WIN], acc[WIN:]
        s = array.array("h")
        s.frombytes(frame)
        peak = max((abs(v) for v in s), default=0) / 32768.0
        rms = (sum(v * v for v in s) / len(s)) ** 0.5 / 32768.0
        peak_hold = max(peak_hold * 0.90, peak)
        sess_peak = max(sess_peak, peak)

        above = peak >= thr_frac
        tag = f"{GREEN}{BOLD}▶ SPEAKING{RESET}" if above else f"{DIM}· silent  {RESET}"
        line = (
            f"\r{bar(dbfs(rms), thr_db)}  "
            f"RMS {rms * 100:5.2f}% ({dbfs(rms):6.1f}dB)  "
            f"PK {peak * 100:5.2f}%  hold {peak_hold * 100:5.2f}%   {tag}\x1b[K"
        )
        sys.stdout.write(line)
        sys.stdout.flush()
except (EOFError, KeyboardInterrupt):
    pass
finally:
    sys.stdout.write("\x1b[?25h\n")  # show cursor
    print(
        f"session peak: {sess_peak * 100:.2f}% ({dbfs(sess_peak):.1f} dBFS)   "
        f"threshold: {thresh_pct:g}% ({thr_db:.1f} dBFS)"
    )
    if sess_peak < thr_frac:
        print(
            "→ your loudest moment never reached the threshold: raise mic gain or "
            "lower vad_threshold_pct / [streaming].silence_threshold_pct."
        )
