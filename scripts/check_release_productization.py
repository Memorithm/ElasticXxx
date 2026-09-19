#!/usr/bin/env python3
"""Fail-closed checks for the BE15f pre-release productization contract."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "docs/release/PRODUCTIZATION-V1.json"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
PUBLIC_CHAIN = {
    "elastic-core",
    "elastic-macros",
    "elastic-eir",
    "elastic-adapters",
    "elastic-runtime",
    "elastic-kv",
    "elastic",
}
EXPECTED_MANIFEST_KEYS = {
    "schema", "scope", "release_line", "workspace_version", "msrv",
    "registry_publication_authorized", "public_rust_boundary", "license_source",
    "qualified_consumers_and_sources", "required_release_documents", "publication_blockers",
}


def fail(message: str) -> None:
    raise SystemExit(f"release-productization: {message}")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    raw = MANIFEST.read_bytes()
    if len(raw) > 64 * 1024:
        fail("productization manifest exceeds 64 KiB")
    data = json.loads(raw)
    if not isinstance(data, dict) or set(data) != EXPECTED_MANIFEST_KEYS:
        fail("manifest uses an unknown or missing top-level field")
    if data["schema"] != 1 or data["scope"] != "elasticxxx-pre-release-productization-v1":
        fail("unsupported productization schema/scope")
    if data["registry_publication_authorized"] is not False:
        fail("pre-release evidence must not authorize registry publication")
    if data["public_rust_boundary"] != "elastic":
        fail("public Rust boundary drifted from elastic facade")

    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
    package = workspace["workspace"]["package"]
    if package["rust-version"] != data["msrv"]:
        fail("manifest MSRV does not match workspace")
    if data["workspace_version"] != "0.1.0" or data["release_line"] != "0.1.x":
        fail("unexpected pre-release version line")

    source = data["license_source"]
    if set(source) != {"repository", "default_branch", "source_commit", "path", "sha256"}:
        fail("license source schema drift")
    if source["repository"] != "Memorithm/scirust" or source["default_branch"] != "master" or source["path"] != "LICENSE.md":
        fail("canonical license provenance drift")
    if not HEX40.fullmatch(source["source_commit"]) or not HEX64.fullmatch(source["sha256"]):
        fail("invalid canonical license identity")
    if sha256(ROOT / "LICENSE.md") != source["sha256"]:
        fail("LICENSE.md no longer matches the recorded SciRust canonical license bytes")

    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=ROOT, text=True
    ))
    workspace_ids = set(metadata["workspace_members"])
    packages = {p["name"]: p for p in metadata["packages"] if p["id"] in workspace_ids}
    if not PUBLIC_CHAIN <= packages.keys():
        fail("public facade dependency chain package is missing")
    for name in PUBLIC_CHAIN:
        p = packages[name]
        if p["version"] != data["workspace_version"]:
            fail(f"{name} version does not match productization manifest")
        if p["rust_version"] != data["msrv"]:
            fail(f"{name} MSRV does not match productization manifest")
        # Cargo metadata represents `publish = false` as an empty allow-list.
        if p.get("publish") != []:
            fail(f"{name} must remain publish=false until a separate release authorization")
        license_file = p.get("license_file") or ""
        if not license_file.endswith("/LICENSE.md"):
            fail(f"{name} must package LICENSE.md")

    consumers = data["qualified_consumers_and_sources"]
    if not isinstance(consumers, list) or len(consumers) != 4:
        fail("expected four reviewed BE15 cross-repository entries")
    repositories = {entry.get("repository") for entry in consumers if isinstance(entry, dict)}
    if repositories != {"Memorithm/BooleanLab", "Memorithm/TDI", "Memorithm/Forge", "Memorithm/ExtremEngine"}:
        fail("cross-repository compatibility set drifted")
    for entry in consumers:
        if not HEX40.fullmatch(entry.get("source_commit", "")):
            fail(f"invalid source commit for {entry.get('repository')}")
        if entry.get("authority") not in {"none", "consumer-owned-actuation-after-fresh-validation"}:
            fail("compatibility evidence may not grant undeclared authority")
        elastic_source = entry.get("elastic_source_commit")
        if elastic_source is not None and not HEX40.fullmatch(elastic_source):
            fail("invalid Elastic consumer source pin")

    docs = data["required_release_documents"]
    if not isinstance(docs, list) or not docs:
        fail("release documents must be explicit")
    for rel in docs:
        path = ROOT / rel
        if not path.is_file() or path.stat().st_size == 0:
            fail(f"required release document missing: {rel}")

    blockers = data["publication_blockers"]
    if not isinstance(blockers, list) or len(blockers) < 5 or any(not isinstance(x, str) or not x for x in blockers):
        fail("publication blockers must remain explicit")

    # Bind two code/data consumers to the same exact-source identities recorded
    # by their actual destination-owned implementation/tests.
    tdi = (ROOT / "crates/elastic-adapters/src/tdi93.rs").read_text()
    if 'TDI93_C3_SOURCE_COMMIT_V1: &str = "7dab3bfa97e74eeff7965cefcada56c59b4322ab"' not in tdi:
        fail("TDI compatibility pin drifted from release manifest")
    booleanlab = (ROOT / "docs/boolean-be15a-booleanlab-bridge.md").read_text()
    if "2646e7ca675d3dcde71abed10c7531e1d36c93b8" not in booleanlab:
        fail("BooleanLab compatibility provenance drifted")

    print("release-productization: pre-release contract valid; publication remains unauthorized")


if __name__ == "__main__":
    main()
