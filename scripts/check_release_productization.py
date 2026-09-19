#!/usr/bin/env python3
"""Fail-closed checks for the BE15f pre-release productization contract."""
from __future__ import annotations

import copy
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
REGISTRY_BY_SOURCE = {
    "elastic-core": "memorithm-elastic-core",
    "elastic-language-syntax": "memorithm-elastic-language-syntax",
    "elastic-macros": "memorithm-elastic-macros",
    "elastic-eir": "memorithm-elastic-eir",
    "elastic-adapters": "memorithm-elastic-adapters",
    "elastic-runtime": "memorithm-elastic-runtime",
    "elastic-kv": "memorithm-elastic-kv",
    "elastic": "memorithm-elastic",
}
SOURCE_BY_REGISTRY = {registry: source for source, registry in REGISTRY_BY_SOURCE.items()}
PUBLIC_CHAIN = set(SOURCE_BY_REGISTRY)
EXPECTED_MANIFEST_KEYS = {
    "schema", "scope", "release_line", "workspace_version", "msrv",
    "registry_publication_authorized", "public_rust_boundary", "license_source",
    "public_package_topology", "qualified_consumers_and_sources",
    "required_release_documents", "publication_blockers",
}

EXPECTED_LICENSE_SOURCE = {
    "repository": "Memorithm/scirust",
    "default_branch": "master",
    "source_commit": "17d1a57f1b9333e1e9a86f4696a9db4a43178d31",
    "path": "LICENSE.md",
    "sha256": "7b3f5edd7c1538affb8477d6699265b5b1c7f7464be0fab113e4e06d031ed72c",
}
EXPECTED_CONSUMERS = {
    "Memorithm/BooleanLab": {
        "name": "BooleanLab exact-vector bridge",
        "repository": "Memorithm/BooleanLab",
        "source_commit": "2646e7ca675d3dcde71abed10c7531e1d36c93b8",
        "kind": "immutable-test-fixture",
        "authority": "none",
    },
    "Memorithm/TDI": {
        "name": "TDI-9.3 non-final carrier bridge",
        "repository": "Memorithm/TDI",
        "source_commit": "7dab3bfa97e74eeff7965cefcada56c59b4322ab",
        "kind": "non-final-representation-adapter",
        "authority": "none",
    },
    "Memorithm/Forge": {
        "name": "Forge optional search bridge review source",
        "repository": "Memorithm/Forge",
        "source_commit": "da68e9703d7a523f5d3703ebedd3de85b39cb153",
        "kind": "candidate-input-contract",
        "authority": "none",
    },
    "Memorithm/ExtremEngine": {
        "name": "ExtremEngine adaptive-quality consumer",
        "repository": "Memorithm/ExtremEngine",
        "source_commit": "51efeb57bd12ee6419cec7e9b530c3b0c485a7a6",
        "elastic_source_commit": "8441991feea3a2aae19f62a8e51c89e7f0d6f969",
        "kind": "real-facade-consumer",
        "authority": "consumer-owned-actuation-after-fresh-validation",
    },
}
EXPECTED_RELEASE_DOCUMENTS = {
    "docs/release/COMPATIBILITY.md",
    "docs/release/PACKAGEABILITY.md",
    "docs/release/MIGRATION-0.1.md",
    "docs/release/CROSS_REPO_COMPATIBILITY.md",
    "docs/release/REGISTRY-NAME-AUDIT.md",
    "docs/release/PACKAGE-NAMING-V1.md",
    "CHANGELOG.md",
    "docs/release/RELEASE-0.1.0.md",
}
EXPECTED_REGISTRY_AUDIT_SHA256 = "b6f5cec969229455db0832d4109acce5fa6e89a76c10ccb60654e0c42bcfecfc"

EXPECTED_PACKAGE_NAMING_SHA256 = "ff1052c8c0302eef8b37ad747115b9d3970f9337e00fd692055bf6420f4d5bd4"
EXPECTED_CHANGELOG_SHA256 = "7f6b7ddf4e25975c023b05c09d1970ef8e609db9d162c2f0cee3212b81ee4050"
EXPECTED_RELEASE_NOTES_SHA256 = "54050102786c38c7f5e1ce281f657b5a95bac975c413e50d6d3ab9014957ebac"
EXPECTED_PUBLICATION_BLOCKERS = {
    "crates_io_name_availability_must_be_rechecked_at_release_time",
    "full_dependency_order_registry_publish_not_executed",
    "clean_registry_downstream_install_not_yet_possible_without_first_publish",
    "non_leaf_package_archives_not_buildable_or_inspectable_until_dependency_order_publish",
}


EXPECTED_PUBLIC_PACKAGE_TOPOLOGY = {
    "registry": "crates.io",
    "topology": "all-facade-dependencies-public-registry-visible",
    "supported_user_boundary": "elastic",
    "registry_packages": [
        {"workspace_package": "elastic-core", "registry_package": "memorithm-elastic-core", "role": "implementation-dependency"},
        {"workspace_package": "elastic-language-syntax", "registry_package": "memorithm-elastic-language-syntax", "role": "implementation-dependency"},
        {"workspace_package": "elastic-macros", "registry_package": "memorithm-elastic-macros", "role": "implementation-dependency"},
        {"workspace_package": "elastic-eir", "registry_package": "memorithm-elastic-eir", "role": "implementation-dependency"},
        {"workspace_package": "elastic-adapters", "registry_package": "memorithm-elastic-adapters", "role": "implementation-dependency"},
        {"workspace_package": "elastic-runtime", "registry_package": "memorithm-elastic-runtime", "role": "implementation-dependency"},
        {"workspace_package": "elastic-kv", "registry_package": "memorithm-elastic-kv", "role": "implementation-dependency"},
        {"workspace_package": "elastic", "registry_package": "memorithm-elastic", "role": "public-facade"},
    ],
    "availability_observed_at": "2026-09-19T05:58:13Z",
    "availability_observation": "legacy-seven-selected-names-returned-404-language-syntax-name-unverified-release-time-recheck-required",
    "manifests_renamed": True,
}

def fail(message: str) -> None:
    raise SystemExit(f"release-productization: {message}")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_pinned_declarations(data: dict[str, object]) -> None:
    topology = data.get("public_package_topology")
    if topology != EXPECTED_PUBLIC_PACKAGE_TOPOLOGY:
        fail("public registry package naming/topology drifted from reviewed decision")
    names = [entry["registry_package"] for entry in topology["registry_packages"]]
    if len(names) != len(set(names)) or any(not name.startswith("memorithm-elastic") for name in names):
        fail("selected registry package names must be unique and Memorithm-prefixed")

    source = data.get("license_source")
    if source != EXPECTED_LICENSE_SOURCE:
        fail("canonical SciRust license source commit/digest drifted")

    consumers = data.get("qualified_consumers_and_sources")
    if not isinstance(consumers, list) or len(consumers) != len(EXPECTED_CONSUMERS):
        fail("expected four reviewed BE15 cross-repository entries")
    if any(not isinstance(entry, dict) for entry in consumers):
        fail("cross-repository entries must be objects")
    actual_by_repo = {entry.get("repository"): entry for entry in consumers}
    if len(actual_by_repo) != len(consumers) or actual_by_repo != EXPECTED_CONSUMERS:
        fail("cross-repository source/kind/authority pins drifted from reviewed tuples")

    docs = data.get("required_release_documents")
    if (
        not isinstance(docs, list)
        or len(docs) != len(EXPECTED_RELEASE_DOCUMENTS)
        or set(docs) != EXPECTED_RELEASE_DOCUMENTS
    ):
        fail("release document set drifted from the canonical pre-release set")

    blockers = data.get("publication_blockers")
    if (
        not isinstance(blockers, list)
        or len(blockers) != len(EXPECTED_PUBLICATION_BLOCKERS)
        or set(blockers) != EXPECTED_PUBLICATION_BLOCKERS
    ):
        fail("publication blocker identities drifted from the unresolved canonical set")


def self_test_pinned_declarations(data: dict[str, object]) -> None:
    def rejected(mutator) -> None:
        candidate = copy.deepcopy(data)
        mutator(candidate)
        try:
            validate_pinned_declarations(candidate)
        except SystemExit:
            return
        fail("self-test accepted tampered productization declarations")

    rejected(lambda d: d["license_source"].update(source_commit="0" * 40))
    rejected(lambda d: d["public_package_topology"].update(manifests_renamed=False))
    rejected(lambda d: d["public_package_topology"]["registry_packages"][0].update(registry_package="elastic-core"))

    def move_authority(d) -> None:
        entries = {entry["repository"]: entry for entry in d["qualified_consumers_and_sources"]}
        entries["Memorithm/BooleanLab"]["authority"] = "consumer-owned-actuation-after-fresh-validation"
        entries["Memorithm/ExtremEngine"]["authority"] = "none"

    rejected(move_authority)
    rejected(lambda d: d["required_release_documents"].remove("docs/release/MIGRATION-0.1.md"))
    rejected(
        lambda d: d.__setitem__(
            "publication_blockers", [f"arbitrary-{i}" for i in range(len(EXPECTED_PUBLICATION_BLOCKERS))]
        )
    )


def main() -> None:
    raw = MANIFEST.read_bytes()
    if len(raw) > 64 * 1024:
        fail("productization manifest exceeds 64 KiB")
    data = json.loads(raw)
    if not isinstance(data, dict) or set(data) != EXPECTED_MANIFEST_KEYS:
        fail("manifest uses an unknown or missing top-level field")
    validate_pinned_declarations(data)
    self_test_pinned_declarations(data)
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
    if not HEX40.fullmatch(source["source_commit"]) or not HEX64.fullmatch(source["sha256"]):
        fail("invalid canonical license identity")
    if sha256(ROOT / "LICENSE.md") != EXPECTED_LICENSE_SOURCE["sha256"]:
        fail("LICENSE.md no longer matches the independently pinned SciRust canonical digest")

    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=ROOT, text=True
    ))
    workspace_ids = set(metadata["workspace_members"])
    packages = {p["name"]: p for p in metadata["packages"] if p["id"] in workspace_ids}
    if not PUBLIC_CHAIN <= packages.keys():
        fail("public facade dependency chain registry package is missing")
    for registry_name in PUBLIC_CHAIN:
        p = packages[registry_name]
        source_name = SOURCE_BY_REGISTRY[registry_name]
        if p["version"] != data["workspace_version"]:
            fail(f"{registry_name} version does not match productization manifest")
        if p["rust_version"] != data["msrv"]:
            fail(f"{registry_name} MSRV does not match productization manifest")
        # Cargo metadata represents `publish = false` as an empty allow-list.
        if p.get("publish") != []:
            fail(f"{registry_name} must remain publish=false until a separate release authorization")
        license_file = p.get("license_file") or ""
        if not license_file.endswith("/LICENSE.md"):
            fail(f"{registry_name} must package LICENSE.md")
        manifest_path = Path(p["manifest_path"]).resolve()
        expected_manifest = (ROOT / "crates" / source_name / "Cargo.toml").resolve()
        if manifest_path != expected_manifest:
            fail(f"{registry_name} moved away from reviewed workspace source path {source_name}")
        expected_lib = source_name.replace("-", "_")
        library_targets = [
            target for target in p.get("targets", [])
            if "lib" in target.get("kind", []) or "proc-macro" in target.get("kind", [])
        ]
        if len(library_targets) != 1 or library_targets[0].get("name") != expected_lib:
            fail(f"{registry_name} must preserve source crate name {expected_lib!r}")
        for dependency in p.get("dependencies", []):
            dep_registry = dependency.get("name")
            if dependency.get("kind") == "dev" or dep_registry not in PUBLIC_CHAIN:
                continue
            expected_alias = SOURCE_BY_REGISTRY[dep_registry]
            if dependency.get("rename") != expected_alias:
                fail(
                    f"{registry_name} -> {dep_registry} must preserve dependency alias "
                    f"{expected_alias!r}; got {dependency.get('rename')!r}"
                )
            if dependency.get("req") != "^0.1.0":
                fail(f"{registry_name} -> {dep_registry} must retain ^0.1.0 registry requirement")

    consumers = data["qualified_consumers_and_sources"]
    actual_by_repo = {entry["repository"]: entry for entry in consumers}
    for repository, entry in actual_by_repo.items():
        if not HEX40.fullmatch(entry["source_commit"]):
            fail(f"invalid source commit for {repository}")
        elastic_source = entry.get("elastic_source_commit")
        if elastic_source is not None and not HEX40.fullmatch(elastic_source):
            fail(f"invalid Elastic consumer source pin for {repository}")

    docs = data["required_release_documents"]
    for rel in docs:
        path = ROOT / rel
        if not path.is_file() or path.stat().st_size == 0:
            fail(f"required release document missing: {rel}")

    registry_audit_path = ROOT / "docs/release/REGISTRY-NAME-AUDIT.md"
    if sha256(registry_audit_path) != EXPECTED_REGISTRY_AUDIT_SHA256:
        fail("registry-name audit digest drifted from the reviewed seven-package evidence")
    package_naming_path = ROOT / "docs/release/PACKAGE-NAMING-V1.md"
    if sha256(package_naming_path) != EXPECTED_PACKAGE_NAMING_SHA256:
        fail("package naming/topology decision digest drifted from reviewed evidence")
    if sha256(ROOT / "CHANGELOG.md") != EXPECTED_CHANGELOG_SHA256:
        fail("0.1.0 changelog digest drifted from the frozen release-candidate record")
    if sha256(ROOT / "docs/release/RELEASE-0.1.0.md") != EXPECTED_RELEASE_NOTES_SHA256:
        fail("0.1.0 release-note digest drifted from the frozen release-candidate record")


    # Bind two code/data consumers to the same exact-source identities recorded
    # by their actual destination-owned implementation/tests.
    tdi = (ROOT / "crates/elastic-adapters/src/tdi93.rs").read_text()
    tdi_commit = EXPECTED_CONSUMERS["Memorithm/TDI"]["source_commit"]
    if f'TDI93_C3_SOURCE_COMMIT_V1: &str = "{tdi_commit}"' not in tdi:
        fail("TDI compatibility pin drifted from reviewed source tuple")
    booleanlab = (ROOT / "docs/boolean-be15a-booleanlab-bridge.md").read_text()
    booleanlab_commit = EXPECTED_CONSUMERS["Memorithm/BooleanLab"]["source_commit"]
    if booleanlab_commit not in booleanlab:
        fail("BooleanLab compatibility provenance drifted")

    print("release-productization: pre-release contract valid; publication remains unauthorized")


if __name__ == "__main__":
    main()
