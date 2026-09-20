#!/usr/bin/env python3
"""Fail-closed source checks for the standalone external-adapter v1 fixture."""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "fixtures" / "external-adapter-v1"
MANIFEST = FIXTURE / "Cargo.toml"
SOURCE = FIXTURE / "src" / "lib.rs"

EXPECTED_DEPENDENCY = {
    "package": "memorithm-elastic",
    "path": "../../crates/elastic",
}
FORBIDDEN_IMPORTS = (
    "elastic_core",
    "elastic_eir",
    "elastic_runtime",
    "elastic_adapters",
    "elastic_kv",
    "elastic_macros",
)
REQUIRED_SYMBOLS = (
    "elastic::external_adapter_v1",
    "impl Observer for FixtureObserver",
    "impl TransactionalActuator for FixtureAdapter",
    "RuntimeError::Validation",
    "misbind_target",
)


def fail(message: str) -> None:
    raise SystemExit(f"external-adapter-v1 conformance check failed: {message}")


def main() -> None:
    manifest = tomllib.loads(MANIFEST.read_text(encoding="utf-8"))
    package = manifest.get("package", {})
    if package.get("publish") is not False:
        fail("fixture must remain publish=false")
    if package.get("rust-version") != "1.89":
        fail("fixture must pin the project MSRV 1.89")

    dependencies = manifest.get("dependencies", {})
    if set(dependencies) != {"elastic"}:
        fail(f"fixture must have exactly one direct dependency named elastic, got {sorted(dependencies)}")
    elastic = dependencies["elastic"]
    if not isinstance(elastic, dict):
        fail("elastic dependency must use explicit package/path metadata")
    for key, expected in EXPECTED_DEPENDENCY.items():
        if elastic.get(key) != expected:
            fail(f"elastic dependency {key} must be {expected!r}, got {elastic.get(key)!r}")

    source = SOURCE.read_text(encoding="utf-8")
    for forbidden in FORBIDDEN_IMPORTS:
        if forbidden in source:
            fail(f"fixture imports implementation crate name {forbidden!r}")
    for required in REQUIRED_SYMBOLS:
        if required not in source:
            fail(f"fixture is missing required contract exercise {required!r}")

    facade = (ROOT / "crates" / "elastic" / "src" / "lib.rs").read_text(encoding="utf-8")
    if "pub mod external_adapter_v1" not in facade:
        fail("facade no longer exports external_adapter_v1")
    if "pub const CONTRACT_VERSION: u16 = 1;" not in facade:
        fail("external adapter v1 contract version changed or disappeared")

    print("external-adapter-v1 source conformance: PASS")


if __name__ == "__main__":
    main()
