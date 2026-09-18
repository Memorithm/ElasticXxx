#!/usr/bin/env python3
from __future__ import annotations

import csv
import hashlib
import math
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE_ROOT = ROOT / "benchmarks" / "be13" / "evidence"
EXPECTED_PATHS = {
    "scalar_if_chain",
    "generic_bool_expr",
    "u64_compiled_guard",
    "multiword_guard",
    "batch_filter",
}
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")


def metadata(path: Path) -> dict[str, str]:
    out: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        if "=" not in line:
            raise AssertionError(f"malformed metadata line in {path}: {line!r}")
        key, value = line.split("=", 1)
        if key in out:
            raise AssertionError(f"duplicate metadata key {key!r} in {path}")
        out[key] = value
    return out


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def validate_evidence(directory: Path) -> None:
    meta_path = directory / "metadata.txt"
    raw_path = directory / "raw.csv"
    frequencies_path = directory / "frequencies.csv"
    sums_path = directory / "SHA256SUMS"
    for path in (meta_path, raw_path, frequencies_path, sums_path):
        if not path.is_file():
            raise AssertionError(f"missing retained evidence file: {path}")

    meta = metadata(meta_path)
    if meta.get("schema") != "elasticxxx-be13-portable-evidence/v1":
        raise AssertionError(f"unsupported evidence schema in {meta_path}")
    if not HEX40.fullmatch(meta.get("source_sha", "")):
        raise AssertionError(f"invalid source_sha in {meta_path}")
    if not HEX64.fullmatch(meta.get("collector_sha256", "")):
        raise AssertionError(f"invalid collector_sha256 in {meta_path}")
    repetitions = int(meta["repetitions"])
    warmup = int(meta["warmup"])
    iterations = int(meta["iterations"])
    if repetitions <= 0 or warmup <= 0 or iterations <= 0:
        raise AssertionError(f"non-positive collection bounds in {meta_path}")
    for field in ("allocations", "memory_peak_bytes", "branch_misses"):
        if meta.get(field) != "unmeasured":
            raise AssertionError(f"{field} must remain explicit unmeasured in {meta_path}")

    with raw_path.open(encoding="utf-8", newline="") as f:
        rows = list(csv.DictReader(f))
    required = {
        "repetition",
        "path",
        "elapsed_ns",
        "evaluations",
        "ns_per_guard",
        "candidates_per_second",
        "stack_bytes_per_guard",
        "allocations",
        "memory_peak_bytes",
        "branch_misses",
        "result",
    }
    if not rows:
        raise AssertionError(f"empty raw CSV in {raw_path}")
    if set(rows[0]) != required:
        raise AssertionError(f"unexpected raw CSV schema in {raw_path}")

    counts = {path: 0 for path in EXPECTED_PATHS}
    seen_repetitions: dict[str, set[int]] = {path: set() for path in EXPECTED_PATHS}
    for row in rows:
        path = row["path"]
        if path not in EXPECTED_PATHS:
            raise AssertionError(f"unexpected benchmark path {path!r} in {raw_path}")
        repetition = int(row["repetition"])
        if not 1 <= repetition <= repetitions:
            raise AssertionError(f"repetition {repetition} out of bounds in {raw_path}")
        if repetition in seen_repetitions[path]:
            raise AssertionError(f"duplicate repetition {repetition} for {path} in {raw_path}")
        seen_repetitions[path].add(repetition)
        counts[path] += 1
        if row["result"] != "True":
            raise AssertionError(f"semantic result drift for {path} in {raw_path}")
        for field in ("allocations", "memory_peak_bytes", "branch_misses"):
            if row[field] != "unmeasured":
                raise AssertionError(f"{field} is not explicit unmeasured in {raw_path}")
        for field in ("elapsed_ns", "evaluations"):
            if int(row[field]) <= 0:
                raise AssertionError(f"non-positive {field} for {path} in {raw_path}")
        for field in ("ns_per_guard", "candidates_per_second", "stack_bytes_per_guard"):
            value = float(row[field])
            if not math.isfinite(value) or value < 0:
                raise AssertionError(f"invalid {field} for {path} in {raw_path}")

    expected_reps = set(range(1, repetitions + 1))
    if any(counts[path] != repetitions for path in EXPECTED_PATHS):
        raise AssertionError(f"incomplete benchmark matrix in {raw_path}: {counts}")
    if any(seen_repetitions[path] != expected_reps for path in EXPECTED_PATHS):
        raise AssertionError(f"missing repetition in {raw_path}")

    with frequencies_path.open(encoding="utf-8", newline="") as f:
        frequency_rows = list(csv.DictReader(f))
    if len(frequency_rows) != repetitions:
        raise AssertionError(f"frequency sample count mismatch in {frequencies_path}")
    for expected_rep, row in enumerate(frequency_rows, start=1):
        if int(row["repetition"]) != expected_rep:
            raise AssertionError(f"frequency repetition order mismatch in {frequencies_path}")
        for field in ("cpu0_frequency_khz_before", "cpu0_frequency_khz_after"):
            value = row[field]
            if value != "unavailable" and int(value) <= 0:
                raise AssertionError(f"invalid {field} in {frequencies_path}")

    recorded: dict[str, str] = {}
    for line in sums_path.read_text(encoding="utf-8").splitlines():
        digest, rel = line.split(maxsplit=1)
        recorded[Path(rel).name] = digest
    for path in (raw_path, frequencies_path, meta_path):
        if recorded.get(path.name) != sha256(path):
            raise AssertionError(f"checksum mismatch for {path}")


if __name__ == "__main__":
    directories = sorted(
        path.parent for path in EVIDENCE_ROOT.glob("*/metadata.txt") if path.is_file()
    )
    if not directories:
        raise SystemExit("no retained BE13 evidence sets found")
    for directory in directories:
        validate_evidence(directory)
        print(f"validated {directory.relative_to(ROOT)}")
