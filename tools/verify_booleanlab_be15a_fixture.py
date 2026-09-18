#!/usr/bin/env python3
"""Verify the pinned BooleanLab BE15a fixture using only Python's stdlib."""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "crates/elastic-core/tests/data/booleanlab"
FIXTURE = DATA / "exact-vectors-v1.tsv"
MANIFEST = DATA / "exact-vectors-v1.manifest"
EXPECTED_SCHEMA = "booleanlab.elasticxxx-exact-vectors-manifest.v1"
EXPECTED_REPOSITORY = "Memorithm/BooleanLab"
EXPECTED_GENERATOR_COMMIT = "7fd929a62cfc219a525b21ff02801fbc8bcb012e"
EXPECTED_GENERATOR_MODULE = "crates/booleanlab-discovery/src/elastic_interop_vectors.rs"
EXPECTED_GENERATOR_MODULE_SHA256 = (
    "85dcf9951b27b9da3aae64e56326e2737f86ae4ca7f2483d67588d396189bef7"
)
EXPECTED_CLAIM_BOUNDARY = "exact-test-vectors-only"


def parse_manifest(text: str) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in text.splitlines():
        if not line or "=" not in line:
            raise ValueError(f"malformed manifest line: {line!r}")
        key, value = line.split("=", 1)
        if not key or key in values:
            raise ValueError(f"invalid or duplicate manifest key: {key!r}")
        values[key] = value
    return values


def is_sha256(value: str) -> bool:
    return len(value) == 64 and all(character in "0123456789abcdef" for character in value)


def verify_bytes(fixture: bytes, manifest_text: str) -> None:
    manifest = parse_manifest(manifest_text)
    required = {
        "schema",
        "vector_schema_version",
        "generator_repository",
        "generator_commit",
        "generator_module",
        "generator_module_sha256",
        "fixture",
        "fixture_sha256",
        "row_order",
        "claim_boundary",
    }
    if set(manifest) != required:
        raise ValueError("manifest keys differ from the pinned v1 contract")
    if manifest["schema"] != EXPECTED_SCHEMA:
        raise ValueError("unexpected manifest schema")
    if manifest["vector_schema_version"] != "1":
        raise ValueError("unexpected vector schema version")
    if manifest["generator_repository"] != EXPECTED_REPOSITORY:
        raise ValueError("unexpected generator repository")
    if manifest["generator_commit"] != EXPECTED_GENERATOR_COMMIT:
        raise ValueError("unexpected generator commit")
    if manifest["generator_module"] != EXPECTED_GENERATOR_MODULE:
        raise ValueError("unexpected generator module")
    generator_module_sha256 = manifest["generator_module_sha256"]
    if not is_sha256(generator_module_sha256):
        raise ValueError("invalid generator module SHA-256")
    if generator_module_sha256 != EXPECTED_GENERATOR_MODULE_SHA256:
        raise ValueError("unexpected generator module SHA-256")
    if manifest["fixture"] != "interop/elasticxxx/exact-vectors-v1.tsv":
        raise ValueError("unexpected source fixture path")
    if manifest["row_order"] != "p0-least-significant-assignment-bit":
        raise ValueError("unexpected assignment ordering")
    if manifest["claim_boundary"] != EXPECTED_CLAIM_BOUNDARY:
        raise ValueError("unexpected claim boundary")
    fixture_sha256 = manifest["fixture_sha256"]
    if not is_sha256(fixture_sha256):
        raise ValueError("invalid fixture SHA-256")
    digest = hashlib.sha256(fixture).hexdigest()
    if digest != fixture_sha256:
        raise ValueError(
            f"fixture SHA-256 mismatch: observed {digest}, expected {fixture_sha256}"
        )


def expect_manifest_rejection(fixture: bytes, manifest: str, old: str, new: str) -> None:
    mutated_manifest = manifest.replace(old, new, 1)
    if mutated_manifest == manifest:
        raise AssertionError(f"self-test mutation did not change manifest: {old!r}")
    try:
        verify_bytes(fixture, mutated_manifest)
    except ValueError:
        return
    raise AssertionError(f"mutated manifest unexpectedly verified: {old!r}")


def self_test() -> None:
    fixture = FIXTURE.read_bytes()
    manifest = MANIFEST.read_text(encoding="utf-8")
    verify_bytes(fixture, manifest)
    mutated = bytearray(fixture)
    mutated[-1] ^= 1
    try:
        verify_bytes(bytes(mutated), manifest)
    except ValueError as error:
        if "SHA-256 mismatch" not in str(error):
            raise
    else:
        raise AssertionError("mutated fixture unexpectedly verified")

    expect_manifest_rejection(
        fixture,
        manifest,
        f"generator_module={EXPECTED_GENERATOR_MODULE}",
        "generator_module=crates/booleanlab-discovery/src/other.rs",
    )
    expect_manifest_rejection(
        fixture,
        manifest,
        f"generator_module_sha256={EXPECTED_GENERATOR_MODULE_SHA256}",
        "generator_module_sha256=" + "0" * 64,
    )
    expect_manifest_rejection(
        fixture,
        manifest,
        f"generator_module_sha256={EXPECTED_GENERATOR_MODULE_SHA256}",
        "generator_module_sha256=not-a-sha256",
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    verify_bytes(FIXTURE.read_bytes(), MANIFEST.read_text(encoding="utf-8"))
    if args.self_test:
        self_test()
    print("BooleanLab BE15a fixture provenance verified")


if __name__ == "__main__":
    main()
