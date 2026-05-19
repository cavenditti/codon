#!/usr/bin/env python3
"""Summarise a codon FM render-trace JSONL file.

Reads the file emitted by `codon --render-trace[=FILE]` (or the
`[diagnostics] render_trace = true` settings field) and prints
p50/p95/p99 distributions per numeric metric plus a keypress -> frame
latency distribution.

Input is JSONL, one event per line, in the shape produced by
`crates/file-manager/src/render/trace.rs`. Stdlib only.

Usage:
    scripts/render-trace-report.py /path/to/trace.jsonl
"""
from __future__ import annotations

import json
import sys
from pathlib import Path


def percentile(values: list[float], pct: float) -> float:
    if not values:
        return float("nan")
    s = sorted(values)
    # Nearest-rank — fine for the per-frame sample sizes we produce (a
    # 60-second navigation session is ~3-4k frames, well above the
    # interpolation-vs-rank noise floor).
    k = max(0, min(len(s) - 1, int(round((pct / 100.0) * (len(s) - 1)))))
    return s[k]


def fmt_dist(name: str, values: list[float], unit: str = "ms") -> str:
    if not values:
        return f"  {name:<28} (no samples)"
    n = len(values)
    p50 = percentile(values, 50)
    p95 = percentile(values, 95)
    p99 = percentile(values, 99)
    mn = min(values)
    mx = max(values)
    mean = sum(values) / n
    return (
        f"  {name:<28} n={n:<6} "
        f"min={mn:8.3f}{unit} p50={p50:8.3f}{unit} "
        f"p95={p95:8.3f}{unit} p99={p99:8.3f}{unit} "
        f"max={mx:8.3f}{unit} mean={mean:8.3f}{unit}"
    )


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(f"usage: {argv[0]} <trace.jsonl>", file=sys.stderr)
        return 2
    path = Path(argv[1])
    if not path.exists():
        print(f"error: {path} does not exist", file=sys.stderr)
        return 1

    keypresses: list[tuple[float, str]] = []
    frame_at: list[float] = []
    prepaint: list[float] = []
    paint: list[float] = []
    draw: list[float] = []
    total: list[float] = []
    rows: list[float] = []
    hits: list[float] = []
    misses: list[float] = []
    previews: list[tuple[float, str]] = []

    with path.open() as fh:
        for line_no, raw in enumerate(fh, start=1):
            raw = raw.strip()
            if not raw:
                continue
            try:
                evt = json.loads(raw)
            except json.JSONDecodeError as err:
                print(
                    f"warning: skipping malformed line {line_no}: {err}",
                    file=sys.stderr,
                )
                continue
            kind = evt.get("t")
            at_ms = float(evt.get("at_ms", 0.0))
            if kind == "keypress":
                keypresses.append((at_ms, evt.get("key", "?")))
            elif kind == "frame_painted":
                frame_at.append(at_ms)
                prepaint.append(float(evt.get("prepaint_ms", 0.0)))
                paint.append(float(evt.get("paint_ms", 0.0)))
                draw.append(float(evt.get("draw_ms", 0.0)))
                total.append(
                    float(evt.get("prepaint_ms", 0.0))
                    + float(evt.get("paint_ms", 0.0))
                    + float(evt.get("draw_ms", 0.0))
                )
                rows.append(float(evt.get("rows_painted", 0)))
                hits.append(float(evt.get("cache_hits", 0)))
                misses.append(float(evt.get("cache_misses", 0)))
            elif kind == "preview_upgraded":
                previews.append((at_ms, evt.get("path", "?")))

    # Keypress -> next frame latency: for every keypress, find the
    # first frame whose at_ms is >= the keypress at_ms.
    latencies: list[float] = []
    frame_idx = 0
    for k_at, _key in keypresses:
        while frame_idx < len(frame_at) and frame_at[frame_idx] < k_at:
            frame_idx += 1
        if frame_idx < len(frame_at):
            latencies.append(frame_at[frame_idx] - k_at)

    print(f"render-trace report: {path}")
    print(f"  events: keypress={len(keypresses)} "
          f"frame_painted={len(frame_at)} preview_upgraded={len(previews)}")
    print()
    print("frame-painted distributions:")
    print(fmt_dist("prepaint_ms", prepaint))
    print(fmt_dist("paint_ms", paint))
    print(fmt_dist("draw_ms", draw))
    print(fmt_dist("total_ms", total))
    print(fmt_dist("rows_painted", rows, unit=""))
    print(fmt_dist("cache_hits", hits, unit=""))
    print(fmt_dist("cache_misses", misses, unit=""))
    print()
    print("keypress -> frame_painted latency:")
    print(fmt_dist("latency_ms", latencies))
    if previews:
        print()
        print(f"preview upgrades ({len(previews)}):")
        for at_ms, p in previews[:10]:
            print(f"  {at_ms:10.3f}ms  {p}")
        if len(previews) > 10:
            print(f"  ... and {len(previews) - 10} more")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
