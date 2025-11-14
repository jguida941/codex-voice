#!/usr/bin/env python3
"""Verify perf smoke voice metrics from log file."""

import sys
import pathlib

def main():
    log_path = pathlib.Path(sys.argv[1])

    if not log_path.exists():
        sys.exit(f"Log file not found: {log_path}")

    lines = [
        line.strip()
        for line in log_path.read_text().splitlines()
        if "voice_metrics|" in line
    ]

    if not lines:
        sys.exit("No voice_metrics lines found")

    latest = lines[-1]
    parts = {}
    for chunk in latest.split("|"):
        if "=" in chunk:
            key, value = chunk.split("=", 1)
            parts[key] = value

    def get_number(key: str) -> float:
        try:
            return float(parts.get(key, "0"))
        except ValueError as exc:
            raise SystemExit(f"Invalid {key} in voice metrics: {latest}") from exc

    capture_ms = get_number("capture_ms")
    speech_ms = get_number("speech_ms")
    silence_tail_ms = get_number("silence_tail_ms")
    frames_dropped = get_number("frames_dropped")
    early_stop = parts.get("early_stop", "")

    if capture_ms <= 0 or capture_ms > 10000:
        sys.exit(f"capture_ms out of bounds: {capture_ms}")
    if speech_ms <= 0:
        sys.exit("speech_ms must be positive")
    if silence_tail_ms > 2000:
        sys.exit(f"silence tail unexpectedly high: {silence_tail_ms}")
    if frames_dropped != 0:
        sys.exit(f"frames_dropped should be zero, got {frames_dropped}")
    if early_stop == "error":
        sys.exit("voice capture ended with error reason")

    print(f"Voice perf smoke metrics valid: {latest}")

if __name__ == "__main__":
    main()
