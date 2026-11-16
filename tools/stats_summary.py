"""Summarises what a run of Dockview measured.

    python tools/stats_summary.py run.csv            # a --log-stats file
    python tools/stats_summary.py --startup LOG      # the start-up milestones
                                                     # of the last run in dockview.log

For a stats file it prints the median and the 95th percentile of each column
(the first seconds are left out: they include start-up work). For the log it
prints how long each step of the start-up took.
"""

import csv
import re
import statistics
import sys
from datetime import datetime
from pathlib import Path

# The columns worth comparing between runs, with how to show them.
COLUMNS = [
    ("received_fps", "fps received"),
    ("shown_fps", "fps shown"),
    ("skipped_per_s", "skipped/s"),
    ("latency_ms", "latency ms"),
    ("cpu_core_percent", "CPU % of a core"),
    ("gpu_ms", "GPU ms/frame"),
    ("memory_mb", "memory MB"),
    ("audio_buffer_ms", "audio buffer ms"),
]
# Seconds at the start of a run that are not counted.
WARM_UP = 3.0


def percentile(values: list[float], share: float) -> float:
    ordered = sorted(values)
    index = min(len(ordered) - 1, round(share * (len(ordered) - 1)))
    return ordered[index]


def summarise_stats(path: Path) -> str:
    rows = list(csv.DictReader(path.open(encoding="utf-8")))
    rows = [r for r in rows if float(r["seconds"]) >= WARM_UP]
    if not rows:
        return f"{path}: no rows after the first {WARM_UP:.0f} s"
    lines = [f"{path.name}: {len(rows)} seconds measured"]
    lines.append(f"  {'':<18}{'median':>10}{'p95':>10}")
    for key, label in COLUMNS:
        values = []
        for row in rows:
            text = row.get(key, "")
            if text not in ("", "-"):
                try:
                    values.append(float(text))
                except ValueError:
                    pass
        if values:
            lines.append(
                f"  {label:<18}{statistics.median(values):>10.2f}{percentile(values, 0.95):>10.2f}"
            )
    return "\n".join(lines)


LINE = re.compile(r"^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}) (\w+)\s+(.*)$")
MILESTONES = [
    "starting",
    "devices listed",
    "application ready",
    "window created",
    "renderer ready",
    "first picture shown",
]


def summarise_startup(path: Path) -> str:
    """The milestones of the last start-up in the log, with the time each step took."""
    last: dict[str, datetime] = {}
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        match = LINE.match(raw)
        if not match:
            continue
        at = datetime.strptime(match.group(1), "%Y-%m-%d %H:%M:%S.%f")
        message = match.group(3)
        for name in MILESTONES:
            if message.endswith(name):
                if name == "starting":
                    last = {}  # a new run: forget the previous one
                last[name] = at
    if "starting" not in last:
        return f"{path}: no start-up found"
    lines = ["start-up (last run in the log):"]
    previous = last["starting"]
    for name in MILESTONES[1:]:
        if name in last:
            step = (last[name] - previous).total_seconds() * 1000
            total = (last[name] - last["starting"]).total_seconds() * 1000
            lines.append(f"  {name:<22}+{step:>7.0f} ms   at {total:>7.0f} ms")
            previous = last[name]
        else:
            lines.append(f"  {name:<22}(not reached)")
    return "\n".join(lines)


def main(argv: list[str]) -> int:
    if len(argv) == 3 and argv[1] == "--startup":
        print(summarise_startup(Path(argv[2])))
        return 0
    if len(argv) == 2 and not argv[1].startswith("-"):
        print(summarise_stats(Path(argv[1])))
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
