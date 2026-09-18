#!/usr/bin/env python3
"""Verify the pinned, non-final TDI-9.3 BE15b fixture using stdlib only."""
from __future__ import annotations

import argparse
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "crates/elastic-adapters/tests/data/tdi9_3"
FIXTURE = DATA / "tdi9.3-c3-carrier-v1.tsv"
MANIFEST = DATA / "tdi9.3-c3-carrier-v1.manifest"
EXPECTED = {
    "schema": "tdi.elasticxxx-c3-carrier-manifest.v1",
    "source_repository": "Memorithm/TDI",
    "source_commit": "7dab3bfa97e74eeff7965cefcada56c59b4322ab",
    "source_pr": "505",
    "source_fixture": "interop/elasticxxx/tdi9.3-c3-carrier-v1.tsv",
    "source_schema": "tdi9.3.elasticxxx-c3-carrier.v1",
    "source_generator": "tdi-ai/examples/boolean_c3_elastic_interop.rs",
    "source_generator_sha256": "5a6fb59f4d2e3902773994aad8781d51625f3c4ec059ee05a05b4de7f364d84a",
    "fixture_sha256": "4dc0d135af3ad99fae9c76c8c7721ab61e22aa36d68b055daedceb1f6d1d7635",
    "claim_boundary": "non-final-representation-only",
    "unknown_policy": "missing-is-elastic-unknown-and-never-tdi-false",
}
EXPECTED_HEADERS = [
    "# schema=tdi9.3.elasticxxx-c3-carrier.v1",
    "# claim_boundary=non-final-representation-only",
    "# predicate_order=BASE_STOP,VERIFY_BEFORE_STOP,CADENCE_DUE,CHECKPOINT_AVAILABLE,REMAINING_WORK,VERIFIER_VIOLATED,VERIFIER_SATISFIED,VERIFIER_INDETERMINATE,VERIFIER_ABSENT",
    "# bit_order=p0-to-p8-left-to-right",
    "# missing_predicate_semantics=not-represented-by-TDI-binary-carrier",
    "# columns=row\tpredicates\tclassification\taction",
]


def parse_manifest(text: str) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in text.splitlines():
        if not line or "=" not in line:
            raise ValueError(f"malformed manifest line: {line!r}")
        key, value = line.split("=", 1)
        if not key or key in values:
            raise ValueError(f"duplicate/invalid manifest key: {key!r}")
        values[key] = value
    return values


def is_sha256(value: str) -> bool:
    return len(value) == 64 and all(character in "0123456789abcdef" for character in value)


def verify_bytes(fixture: bytes, manifest_text: str) -> None:
    manifest = parse_manifest(manifest_text)
    if manifest != EXPECTED:
        raise ValueError("TDI-9.3 BE15b manifest differs from pinned contract")
    for key in ("source_generator_sha256", "fixture_sha256"):
        if not is_sha256(manifest[key]):
            raise ValueError(f"invalid SHA-256 in {key}")
    observed = hashlib.sha256(fixture).hexdigest()
    if observed != manifest["fixture_sha256"]:
        raise ValueError(f"fixture SHA-256 mismatch: observed {observed}")

    text = fixture.decode("utf-8", errors="strict")
    lines = text.splitlines()
    if lines[: len(EXPECTED_HEADERS)] != EXPECTED_HEADERS:
        raise ValueError("fixture header differs from pinned representation contract")
    rows = lines[len(EXPECTED_HEADERS) :]
    if len(rows) != 512:
        raise ValueError(f"expected 512 carrier rows, observed {len(rows)}")
    counts = {"ACTION": 0, "UNRECOVERABLE": 0, "INVALID": 0}
    for expected_index, line in enumerate(rows):
        fields = line.split("\t")
        if len(fields) != 4 or fields[0] != str(expected_index):
            raise ValueError(f"malformed or reordered row {expected_index}")
        bits, classification, action = fields[1], fields[2], fields[3]
        if len(bits) != 9 or any(bit not in "01" for bit in bits):
            raise ValueError(f"invalid predicate bits at row {expected_index}")
        if classification not in counts:
            raise ValueError(f"unknown classification at row {expected_index}")
        if (classification == "ACTION") != (action != "-"):
            raise ValueError(f"action/classification mismatch at row {expected_index}")
        counts[classification] += 1
    if counts != {"ACTION": 120, "UNRECOVERABLE": 8, "INVALID": 384}:
        raise ValueError(f"unexpected source partition {counts}")


def expect_rejection(fixture: bytes, manifest: str, old: str, new: str) -> None:
    mutated = manifest.replace(old, new, 1)
    if mutated == manifest:
        raise AssertionError(f"self-test mutation did not change manifest: {old!r}")
    try:
        verify_bytes(fixture, mutated)
    except ValueError:
        return
    raise AssertionError(f"mutated manifest unexpectedly verified: {old!r}")


def self_test() -> None:
    fixture = FIXTURE.read_bytes()
    manifest = MANIFEST.read_text(encoding="utf-8")
    verify_bytes(fixture, manifest)

    mutated_fixture = bytearray(fixture)
    mutated_fixture[-2] = ord("1") if mutated_fixture[-2] != ord("1") else ord("0")
    try:
        verify_bytes(bytes(mutated_fixture), manifest)
    except ValueError:
        pass
    else:
        raise AssertionError("mutated TDI fixture unexpectedly verified")

    expect_rejection(
        fixture,
        manifest,
        f"source_commit={EXPECTED['source_commit']}",
        "source_commit=" + "0" * 40,
    )
    expect_rejection(
        fixture,
        manifest,
        f"source_generator_sha256={EXPECTED['source_generator_sha256']}",
        "source_generator_sha256=" + "0" * 64,
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    verify_bytes(FIXTURE.read_bytes(), MANIFEST.read_text(encoding="utf-8"))
    if args.self_test:
        self_test()
    print("TDI-9.3 BE15b fixture provenance verified")


if __name__ == "__main__":
    main()
