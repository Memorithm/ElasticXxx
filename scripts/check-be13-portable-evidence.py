#!/usr/bin/env python3
from __future__ import annotations

import base64
import csv
import hashlib
import json
import math
import re
import subprocess
from collections import Counter
from pathlib import Path

from be13_timing_stability import (
    BLOCK_SIZE as TIMING_BLOCK_SIZE,
    MAX_BLOCK_MEDIAN_SPREAD_RATIO as TIMING_MAX_SPREAD,
    MIN_REPETITIONS as TIMING_MIN_REPETITIONS,
    analyze as analyze_timing_stability,
)

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE_ROOT = ROOT / "benchmarks" / "be13" / "evidence"
EXPECTED_PATHS = [
    "scalar_if_chain",
    "generic_bool_expr",
    "u64_compiled_guard",
    "multiword_guard",
    "batch_filter",
]
EXPECTED_PATH_SET = set(EXPECTED_PATHS)
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


def git_blob_sha256(commit: str, path: str) -> str:
    proc = subprocess.run(
        ["git", "show", f"{commit}:{path}"],
        cwd=ROOT,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if proc.returncode != 0:
        raise AssertionError(
            f"cannot read {path} from evidence source {commit}: "
            f"{proc.stderr.decode(errors='replace').strip()}"
        )
    return hashlib.sha256(proc.stdout).hexdigest()


def require_source_commit(meta: dict[str, str], meta_path: Path) -> str:
    source_sha = meta.get("source_sha", "")
    if not HEX40.fullmatch(source_sha):
        raise AssertionError(f"invalid source_sha in {meta_path}")
    proc = subprocess.run(
        ["git", "cat-file", "-e", f"{source_sha}^{{commit}}"],
        cwd=ROOT,
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    if proc.returncode != 0:
        raise AssertionError(f"source_sha {source_sha} is not available as a commit")

    source_ref = meta.get("source_ref")
    if source_ref and source_ref != "none":
        if not source_ref.startswith("refs/tags/"):
            raise AssertionError(f"source_ref must be a permanent refs/tags/... ref in {meta_path}")
        resolved = subprocess.run(
            ["git", "rev-parse", f"{source_ref}^{{commit}}"],
            cwd=ROOT,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        if resolved.returncode != 0:
            raise AssertionError(f"source_ref {source_ref} is not available in {meta_path}")
        if resolved.stdout.strip() != source_sha:
            raise AssertionError(
                f"source_ref {source_ref} resolves to {resolved.stdout.strip()}, expected {source_sha}"
            )
    return source_sha


def validate_historical_tool_hashes(meta: dict[str, str], source_sha: str, *, v2: bool) -> None:
    collector = meta.get("collector_sha256", "")
    if not HEX64.fullmatch(collector):
        raise AssertionError("invalid collector_sha256")
    historical = git_blob_sha256(source_sha, "scripts/collect-be13-portable-evidence.sh")
    if collector != historical:
        raise AssertionError(
            f"collector hash {collector} does not match source {source_sha} blob {historical}"
        )
    if v2:
        helper = meta.get("metrics_helper_sha256", "")
        if not HEX64.fullmatch(helper):
            raise AssertionError("invalid metrics_helper_sha256")
        historical_helper = git_blob_sha256(source_sha, "tools/be13/process_metrics.c")
        if helper != historical_helper:
            raise AssertionError(
                f"metrics helper hash {helper} does not match source {source_sha} blob {historical_helper}"
            )
        analyzer = meta.get("timing_analyzer_sha256", "")
        if not HEX64.fullmatch(analyzer):
            raise AssertionError("invalid timing_analyzer_sha256")
        historical_analyzer = git_blob_sha256(source_sha, "scripts/be13_timing_stability.py")
        if analyzer != historical_analyzer:
            raise AssertionError(
                f"timing analyzer hash {analyzer} does not match source {source_sha} blob {historical_analyzer}"
            )


def validate_checksums(directory: Path, filenames: list[str]) -> None:
    sums_path = directory / "SHA256SUMS"
    if not sums_path.is_file():
        raise AssertionError(f"missing retained evidence file: {sums_path}")
    recorded: dict[str, str] = {}
    for line in sums_path.read_text(encoding="utf-8").splitlines():
        digest, rel = line.split(maxsplit=1)
        name = Path(rel).name
        if name in recorded:
            raise AssertionError(f"duplicate checksum entry for {name} in {sums_path}")
        if not HEX64.fullmatch(digest):
            raise AssertionError(f"invalid checksum for {name} in {sums_path}")
        recorded[name] = digest
    for filename in filenames:
        path = directory / filename
        if not path.is_file():
            raise AssertionError(f"missing retained evidence file: {path}")
        if recorded.get(filename) != sha256(path):
            raise AssertionError(f"checksum mismatch for {path}")


def read_raw_rows(raw_path: Path, repetitions: int) -> list[dict[str, str]]:
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
        if path not in EXPECTED_PATH_SET:
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
                raise AssertionError(
                    f"timing CSV {field} is not explicit unmeasured in {raw_path}"
                )
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
    return rows


def expected_rotating_sequence(repetitions: int) -> list[tuple[int, str]]:
    out: list[tuple[int, str]] = []
    for repetition in range(1, repetitions + 1):
        offset = (repetition - 1) % len(EXPECTED_PATHS)
        for step in range(len(EXPECTED_PATHS)):
            out.append((repetition, EXPECTED_PATHS[(offset + step) % len(EXPECTED_PATHS)]))
    return out


def validate_v1(directory: Path, meta: dict[str, str]) -> None:
    meta_path = directory / "metadata.txt"
    require_source_commit(meta, meta_path)
    collector = meta.get("collector_sha256", "")
    if not HEX64.fullmatch(collector):
        raise AssertionError(f"invalid collector_sha256 in {meta_path}")
    repetitions = int(meta["repetitions"])
    warmup = int(meta["warmup"])
    iterations = int(meta["iterations"])
    if repetitions <= 0 or warmup <= 0 or iterations <= 0:
        raise AssertionError(f"non-positive collection bounds in {meta_path}")
    for field in ("allocations", "memory_peak_bytes", "branch_misses"):
        if meta.get(field) != "unmeasured":
            raise AssertionError(f"{field} must remain explicit unmeasured in {meta_path}")

    raw_path = directory / "raw.csv"
    read_raw_rows(raw_path, repetitions)

    frequencies_path = directory / "frequencies.csv"
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

    validate_checksums(directory, ["raw.csv", "frequencies.csv", "metadata.txt"])


def validate_v2(directory: Path, meta: dict[str, str]) -> None:
    meta_path = directory / "metadata.txt"
    source_sha = require_source_commit(meta, meta_path)
    validate_historical_tool_hashes(meta, source_sha, v2=True)

    repetitions = int(meta["repetitions"])
    metric_repetitions = int(meta["metric_repetitions"])
    warmup = int(meta["warmup"])
    iterations = int(meta["iterations"])
    sample_interval_ms = int(meta["frequency_sample_interval_ms"])
    sample_count = int(meta["frequency_sample_count"])
    if min(repetitions, metric_repetitions, warmup, iterations, sample_interval_ms) <= 0:
        raise AssertionError(f"non-positive v2 collection bound in {meta_path}")
    if repetitions < TIMING_MIN_REPETITIONS or repetitions % TIMING_BLOCK_SIZE != 0:
        raise AssertionError(
            f"v2 timing stability requires at least {TIMING_MIN_REPETITIONS} repetitions "
            f"and divisibility by {TIMING_BLOCK_SIZE}"
        )
    if meta.get("allocations") != "unmeasured":
        raise AssertionError("v2 allocations must remain explicit unmeasured")

    codegen_profile = meta.get("codegen_profile")
    codegen_attestation = meta.get("codegen_attestation")

    def decoded(name: str) -> str:
        value = meta.get(name)
        if value is None:
            raise AssertionError(f"missing attested metadata field {name}")
        try:
            return base64.b64decode(value, validate=True).decode("utf-8")
        except Exception as error:
            raise AssertionError(f"invalid base64 metadata field {name}") from error

    if codegen_attestation is None:
        # Historical BE13e files may carry post-hoc labels. They remain archival
        # evidence but are not sufficient for codegen attribution.
        if codegen_profile is not None and codegen_profile not in {"portable", "native"}:
            raise AssertionError(f"unsupported legacy codegen_profile {codegen_profile!r}")
    elif codegen_attestation == "cargo-rustc-print-cfg-v1":
        if codegen_profile not in {"portable", "native"}:
            raise AssertionError(f"attested codegen_profile must be portable or native, got {codegen_profile!r}")
        if meta.get("source_ref", "none") == "none":
            raise AssertionError("attested codegen evidence requires a permanent source_ref")

        rustflags = decoded("rustflags_base64")
        encoded = decoded("cargo_encoded_rustflags_base64")
        target_flags = decoded("target_rustflags_base64")
        expected_profile = (
            "portable"
            if not rustflags and not encoded and not target_flags
            else "native"
            if rustflags == "-C target-cpu=native" and not encoded and not target_flags
            else "custom"
        )
        if expected_profile != codegen_profile:
            raise AssertionError(
                f"attested codegen profile {codegen_profile!r} disagrees with captured rustflags ({expected_profile})"
            )

        compiler_cfg = directory / "compiler_cfg.txt"
        cargo_inventory = directory / "cargo_config_inventory.txt"
        for path, field in (
            (compiler_cfg, "compiler_cfg_sha256"),
            (cargo_inventory, "cargo_config_inventory_sha256"),
        ):
            if not path.is_file():
                raise AssertionError(f"missing codegen attestation file: {path}")
            if meta.get(field) != sha256(path):
                raise AssertionError(f"{field} does not match {path}")
        inventory_text = cargo_inventory.read_text(encoding="utf-8").strip()
        if inventory_text != "none":
            raise AssertionError(
                "qualified portable/native codegen attestation requires no discovered Cargo config files; "
                "use a separately reviewed custom-profile contract when config-injected rustflags exist"
            )
        cfg_lines = set(compiler_cfg.read_text(encoding="utf-8").splitlines())
        if not any(line.startswith('target_arch=') for line in cfg_lines):
            raise AssertionError("compiler_cfg.txt lacks target_arch")
        if codegen_profile == "native" and meta.get("rustc_host", "").startswith("aarch64-"):
            if 'target_feature="sve"' not in cfg_lines or 'target_feature="sve2"' not in cfg_lines:
                raise AssertionError("AArch64 native attestation lacks effective SVE/SVE2 target features")
    elif codegen_attestation == "cargo-build-context-v2":
        if codegen_profile not in {"portable", "native", "custom"}:
            raise AssertionError(f"invalid v2 codegen_profile {codegen_profile!r}")
        if meta.get("source_ref", "none") == "none":
            raise AssertionError("v2 codegen attestation requires a permanent source_ref")

        rustflags = decoded("rustflags_base64")
        encoded = decoded("cargo_encoded_rustflags_base64")
        encoded_present_raw = meta.get("cargo_encoded_rustflags_present")
        if encoded_present_raw not in {"true", "false"}:
            raise AssertionError("cargo_encoded_rustflags_present must be true or false")
        encoded_present = encoded_present_raw == "true"
        target_flags = decoded("target_rustflags_base64")
        build_rustflags = decoded("cargo_build_rustflags_base64")
        build_target = decoded("cargo_build_target_base64")
        cargo_incremental = decoded("cargo_incremental_base64")
        rustc_override = decoded("rustc_override_base64")
        rustc_wrapper = decoded("rustc_wrapper_base64")
        rustc_workspace_wrapper = decoded("rustc_workspace_wrapper_base64")
        target_linker = decoded("target_linker_base64")

        compiler_cfg = directory / "compiler_cfg.txt"
        cargo_inventory = directory / "cargo_config_inventory.txt"
        build_env_inventory = directory / "build_env_inventory.txt"
        for path, field in (
            (compiler_cfg, "compiler_cfg_sha256"),
            (cargo_inventory, "cargo_config_inventory_sha256"),
            (build_env_inventory, "build_env_inventory_sha256"),
        ):
            if not path.is_file():
                raise AssertionError(f"missing v2 codegen attestation file: {path}")
            if meta.get(field) != sha256(path):
                raise AssertionError(f"{field} does not match {path}")

        cargo_inventory_text = cargo_inventory.read_text(encoding="utf-8").strip()
        build_env_text = build_env_inventory.read_text(encoding="utf-8").strip()
        configs_present = cargo_inventory_text != "none"
        cargo_profile_overrides_present = build_env_text != "none"
        if meta.get("cargo_configs_present") != str(configs_present).lower():
            raise AssertionError("cargo_configs_present disagrees with cargo_config_inventory.txt")
        if meta.get("cargo_profile_overrides_present") != str(cargo_profile_overrides_present).lower():
            raise AssertionError("cargo_profile_overrides_present disagrees with build_env_inventory.txt")

        clean_context = not any(
            (
                encoded,
                target_flags,
                build_rustflags,
                build_target,
                cargo_incremental,
                rustc_override,
                rustc_wrapper,
                rustc_workspace_wrapper,
                target_linker,
            )
        ) and not encoded_present and not configs_present and not cargo_profile_overrides_present
        expected_profile = (
            "portable"
            if clean_context and not rustflags
            else "native"
            if clean_context and rustflags == "-C target-cpu=native"
            else "custom"
        )
        if expected_profile != codegen_profile:
            raise AssertionError(
                f"v2 codegen profile {codegen_profile!r} disagrees with captured build context ({expected_profile})"
            )

        cfg_lines = set(compiler_cfg.read_text(encoding="utf-8").splitlines())
        if not any(line.startswith('target_arch=') for line in cfg_lines):
            raise AssertionError("compiler_cfg.txt lacks target_arch")
        if codegen_profile == "native" and meta.get("rustc_host", "").startswith("aarch64-"):
            if 'target_feature="sve"' not in cfg_lines or 'target_feature="sve2"' not in cfg_lines:
                raise AssertionError("AArch64 native attestation lacks effective SVE/SVE2 target features")
    else:
        raise AssertionError(f"unsupported codegen_attestation {codegen_attestation!r}")

    raw_path = directory / "raw.csv"
    raw_rows = read_raw_rows(raw_path, repetitions)
    actual_sequence = [(int(row["repetition"]), row["path"]) for row in raw_rows]
    if actual_sequence != expected_rotating_sequence(repetitions):
        raise AssertionError(f"timing path order is not deterministic rotation in {raw_path}")

    timing_path = directory / "timing_stability.json"
    if not timing_path.is_file():
        raise AssertionError(f"missing retained timing stability file: {timing_path}")
    timing_recorded = json.loads(timing_path.read_text(encoding="utf-8"))
    timing_expected = analyze_timing_stability(raw_path, repetitions)
    if timing_recorded != timing_expected:
        raise AssertionError(f"timing stability summary does not match raw rows in {timing_path}")
    if meta.get("timing_stable") not in {"true", "false"}:
        raise AssertionError("timing_stable must be true or false")
    if (meta.get("timing_stable") == "true") != bool(timing_expected["stable"]):
        raise AssertionError("metadata timing_stable disagrees with timing stability summary")
    if meta.get("timing_stability_minimum_repetitions") != str(TIMING_MIN_REPETITIONS):
        raise AssertionError("unexpected timing stability minimum repetition contract")
    if meta.get("timing_stability_block_size") != str(TIMING_BLOCK_SIZE):
        raise AssertionError("unexpected timing stability block-size contract")
    if float(meta.get("timing_stability_max_block_median_spread_ratio", "nan")) != TIMING_MAX_SPREAD:
        raise AssertionError("unexpected timing stability spread limit")
    if not math.isclose(
        float(meta.get("timing_worst_block_median_spread_ratio", "nan")),
        float(timing_expected["worst_block_median_spread_ratio"]),
        rel_tol=0.0,
        abs_tol=5e-10,
    ):
        raise AssertionError("metadata worst block-median spread disagrees with timing summary")

    frequencies_path = directory / "frequencies.csv"
    with frequencies_path.open(encoding="utf-8", newline="") as f:
        freq_rows = list(csv.DictReader(f))
    if not freq_rows or set(freq_rows[0]) != {
        "repetition",
        "path",
        "cpu_frequency_khz_before",
        "cpu_frequency_khz_after",
    }:
        raise AssertionError(f"unexpected v2 frequency CSV schema in {frequencies_path}")
    freq_sequence = [(int(row["repetition"]), row["path"]) for row in freq_rows]
    if freq_sequence != expected_rotating_sequence(repetitions):
        raise AssertionError(f"frequency path order mismatch in {frequencies_path}")
    for row in freq_rows:
        for field in ("cpu_frequency_khz_before", "cpu_frequency_khz_after"):
            value = row[field]
            if value != "unavailable" and (not value.isdigit() or int(value) <= 0):
                raise AssertionError(f"invalid {field} in {frequencies_path}")

    samples_path = directory / "frequency_samples.csv"
    with samples_path.open(encoding="utf-8", newline="") as f:
        samples = list(csv.DictReader(f))
    if samples and set(samples[0]) != {"sample", "monotonic_ns", "frequency_khz"}:
        raise AssertionError(f"unexpected frequency sample schema in {samples_path}")
    if len(samples) != sample_count:
        raise AssertionError(f"metadata frequency sample count mismatch in {samples_path}")
    previous_ns = -1
    for expected_index, row in enumerate(samples, start=1):
        if int(row["sample"]) != expected_index:
            raise AssertionError(f"frequency sample index mismatch in {samples_path}")
        monotonic_ns = int(row["monotonic_ns"])
        if monotonic_ns <= previous_ns:
            raise AssertionError(f"frequency sample timestamps are not increasing in {samples_path}")
        previous_ns = monotonic_ns
        value = row["frequency_khz"]
        if value != "unavailable" and (not value.isdigit() or int(value) <= 0):
            raise AssertionError(f"invalid frequency sample in {samples_path}")

    process_path = directory / "process_metrics.csv"
    with process_path.open(encoding="utf-8", newline="") as f:
        process_rows = list(csv.DictReader(f))
    expected_process_fields = {
        "repetition",
        "path",
        "evaluations",
        "branch_misses",
        "branch_misses_per_guard",
        "max_rss_bytes",
        "result",
        "counter_status",
        "counter_errno",
    }
    if process_rows and set(process_rows[0]) != expected_process_fields:
        raise AssertionError(f"unexpected process metrics schema in {process_path}")

    helper_status = meta.get("process_metrics_helper")
    if helper_status == "available":
        if len(process_rows) != metric_repetitions * len(EXPECTED_PATHS):
            raise AssertionError(f"incomplete process metric matrix in {process_path}")
        process_sequence = [(int(row["repetition"]), row["path"]) for row in process_rows]
        if process_sequence != expected_rotating_sequence(metric_repetitions):
            raise AssertionError(f"process metric path order mismatch in {process_path}")
        for row in process_rows:
            if row["result"] != "True" or int(row["evaluations"]) <= 0:
                raise AssertionError(f"invalid process metric semantic row in {process_path}")
            if int(row["max_rss_bytes"]) <= 0:
                raise AssertionError(f"non-positive process max RSS in {process_path}")
            if row["counter_status"] == "measured":
                if not row["branch_misses"].isdigit():
                    raise AssertionError(f"measured branch count is not numeric in {process_path}")
                value = float(row["branch_misses_per_guard"])
                if not math.isfinite(value) or value < 0:
                    raise AssertionError(f"invalid branch miss rate in {process_path}")
                if int(row["counter_errno"]) != 0:
                    raise AssertionError(f"measured branch counter has nonzero errno in {process_path}")
            else:
                if row["branch_misses"] != "unmeasured" or row["branch_misses_per_guard"] != "unmeasured":
                    raise AssertionError(f"unavailable branch counter must remain unmeasured in {process_path}")
    elif helper_status == "unavailable":
        if process_rows:
            raise AssertionError(f"process metrics exist despite unavailable helper in {process_path}")
    else:
        raise AssertionError(f"invalid process_metrics_helper value {helper_status!r}")

    qualified = meta.get("comparison_qualified") == "true"
    if meta.get("comparison_qualified") not in {"true", "false"}:
        raise AssertionError("comparison_qualified must be true or false")
    if qualified:
        if meta.get("frequency_control") != "lock-max":
            raise AssertionError("qualified v2 evidence requires lock-max frequency control")
        if meta.get("frequency_stable") != "true":
            raise AssertionError("qualified v2 evidence requires stable frequency samples")
        if meta.get("frequency_policy_restored") != "true":
            raise AssertionError("qualified v2 evidence requires restored CPUFreq policy")
        if meta.get("timing_stable") != "true":
            raise AssertionError("qualified v2 evidence requires preregistered timing stability")
        target_text = meta.get("frequency_lock_target_khz", "")
        if not target_text.isdigit() or int(target_text) <= 0:
            raise AssertionError("qualified v2 evidence requires numeric frequency target")
        target = int(target_text)
        if len(samples) < 10:
            raise AssertionError("qualified v2 evidence requires at least 10 continuous frequency samples")
        if any(row["frequency_khz"] != target_text for row in samples):
            raise AssertionError("qualified v2 evidence has a continuous frequency sample outside target")
        if any(
            row[field] != target_text
            for row in freq_rows
            for field in ("cpu_frequency_khz_before", "cpu_frequency_khz_after")
        ):
            raise AssertionError("qualified v2 evidence has an edge frequency sample outside target")
        if meta.get("frequency_after_governor") != meta.get("frequency_original_governor"):
            raise AssertionError("CPUFreq governor was not restored")
        if meta.get("frequency_after_min_khz") != meta.get("frequency_original_min_khz"):
            raise AssertionError("CPUFreq minimum was not restored")
        if meta.get("frequency_after_max_khz") != meta.get("frequency_original_max_khz"):
            raise AssertionError("CPUFreq maximum was not restored")

    if meta.get("branch_misses") == "measured_whole_process_user_space":
        if not process_rows or any(row["counter_status"] != "measured" for row in process_rows):
            raise AssertionError("metadata claims measured branch misses without complete direct counters")
    elif meta.get("branch_misses") != "unmeasured":
        raise AssertionError("unexpected v2 branch_misses metadata value")

    if meta.get("memory_peak_bytes") == "measured_whole_process_ru_maxrss":
        if not process_rows or any(int(row["max_rss_bytes"]) <= 0 for row in process_rows):
            raise AssertionError("metadata claims measured peak RSS without complete measurements")
    elif meta.get("memory_peak_bytes") != "unmeasured":
        raise AssertionError("unexpected v2 memory_peak_bytes metadata value")

    validate_checksums(
        directory,
        [
            "raw.csv",
            "frequencies.csv",
            "frequency_samples.csv",
            "process_metrics.csv",
            "timing_stability.json",
            *(
                ["compiler_cfg.txt", "cargo_config_inventory.txt"]
                if codegen_attestation == "cargo-rustc-print-cfg-v1"
                else ["compiler_cfg.txt", "cargo_config_inventory.txt", "build_env_inventory.txt"]
                if codegen_attestation == "cargo-build-context-v2"
                else []
            ),
            "metadata.txt",
            "README.md",
        ],
    )


def validate_evidence(directory: Path) -> None:
    meta_path = directory / "metadata.txt"
    if not meta_path.is_file():
        raise AssertionError(f"missing retained evidence file: {meta_path}")
    meta = metadata(meta_path)
    schema = meta.get("schema")
    if schema == "elasticxxx-be13-portable-evidence/v1":
        validate_v1(directory, meta)
    elif schema == "elasticxxx-be13-portable-evidence/v2":
        validate_v2(directory, meta)
    else:
        raise AssertionError(f"unsupported evidence schema {schema!r} in {meta_path}")


if __name__ == "__main__":
    directories = sorted(
        path.parent for path in EVIDENCE_ROOT.glob("*/metadata.txt") if path.is_file()
    )
    if not directories:
        raise SystemExit("no retained BE13 evidence sets found")
    for directory in directories:
        validate_evidence(directory)
        print(f"validated {directory.relative_to(ROOT)}")
