#!/usr/bin/env python3
"""Deterministic BE13 timing-drift qualification over repeated raw rows.

This is an engineering stability gate, not a statistical significance test.
It intentionally uses contiguous block medians so isolated scheduler/interrupt
outliers remain visible in raw data but do not get deleted post hoc.
"""
from __future__ import annotations

import argparse
import csv
import json
import math
import statistics
from pathlib import Path

EXPECTED_PATHS = (
    "scalar_if_chain",
    "generic_bool_expr",
    "u64_compiled_guard",
    "multiword_guard",
    "batch_filter",
)
MIN_REPETITIONS = 30
BLOCK_SIZE = 5
MAX_BLOCK_MEDIAN_SPREAD_RATIO = 0.10


def analyze(raw_path: Path, repetitions: int) -> dict[str, object]:
    if repetitions < MIN_REPETITIONS:
        raise ValueError(
            f"timing stability requires at least {MIN_REPETITIONS} repetitions; got {repetitions}"
        )
    if repetitions % BLOCK_SIZE != 0:
        raise ValueError(
            f"timing stability repetitions must be divisible by block size {BLOCK_SIZE}; got {repetitions}"
        )

    with raw_path.open(encoding="utf-8", newline="") as f:
        rows = list(csv.DictReader(f))

    paths: dict[str, dict[str, object]] = {}
    stable = True
    worst_spread = 0.0
    for path in EXPECTED_PATHS:
        by_rep: dict[int, float] = {}
        for row in rows:
            if row.get("path") != path:
                continue
            repetition = int(row["repetition"])
            if repetition in by_rep:
                raise ValueError(f"duplicate repetition {repetition} for {path}")
            value = float(row["ns_per_guard"])
            if not math.isfinite(value) or value <= 0:
                raise ValueError(f"invalid ns_per_guard {value!r} for {path}")
            by_rep[repetition] = value
        expected = set(range(1, repetitions + 1))
        if set(by_rep) != expected:
            raise ValueError(f"incomplete repetition set for {path}")

        values = [by_rep[index] for index in range(1, repetitions + 1)]
        median = statistics.median(values)
        block_medians = [
            statistics.median(values[start : start + BLOCK_SIZE])
            for start in range(0, repetitions, BLOCK_SIZE)
        ]
        spread_ratio = (max(block_medians) - min(block_medians)) / median
        path_stable = spread_ratio <= MAX_BLOCK_MEDIAN_SPREAD_RATIO
        stable = stable and path_stable
        worst_spread = max(worst_spread, spread_ratio)
        outliers_1_5x = sum(value > median * 1.5 or value < median / 1.5 for value in values)
        paths[path] = {
            "median_ns_per_guard": median,
            "block_medians_ns_per_guard": block_medians,
            "block_median_spread_ratio": spread_ratio,
            "outliers_outside_1_5x_median": outliers_1_5x,
            "stable": path_stable,
        }

    return {
        "schema": "elasticxxx-be13-timing-stability/v1",
        "repetitions": repetitions,
        "minimum_repetitions": MIN_REPETITIONS,
        "block_size": BLOCK_SIZE,
        "maximum_block_median_spread_ratio": MAX_BLOCK_MEDIAN_SPREAD_RATIO,
        "method": "contiguous_nonoverlapping_block_medians_no_posthoc_row_deletion",
        "stable": stable,
        "worst_block_median_spread_ratio": worst_spread,
        "paths": paths,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("raw_csv", type=Path)
    parser.add_argument("--repetitions", type=int, required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = analyze(args.raw_csv, args.repetitions)
    encoded = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(encoded, encoding="utf-8")
    else:
        print(encoded, end="")


if __name__ == "__main__":
    main()
