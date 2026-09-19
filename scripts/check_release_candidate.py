#!/usr/bin/env python3
"""Verify the retained qualified 0.1.0 baseline without freezing development HEAD.

The baseline is a historical, exact-head qualified Git object. Current `main` may
advance after that point. This checker proves that the baseline commit/tree and
payload digest remain available and unchanged while separately requiring the
current workspace to remain non-publishable. It never mutates or queries a
package registry.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "docs/release/RELEASE-CANDIDATE-V1.json"
PRODUCTIZATION = ROOT / "docs/release/PRODUCTIZATION-V1.json"
EXCLUDED_PAYLOAD_PATHS = {"docs/release/RELEASE-CANDIDATE-V1.json"}
HEX40 = re.compile(r"^[0-9a-f]{40}$")
EXPECTED_TOP_LEVEL_KEYS = {
    "schema",
    "scope",
    "release_version",
    "release_line",
    "msrv",
    "baseline_source_commit",
    "baseline_source_tree",
    "payload_digest_algorithm",
    "payload_sha256",
    "payload_exclusions",
    "baseline_qualification",
    "publication_suspended",
    "registry_publication_authorized",
    "registry_mutation_authorized",
    "network_registry_queries_authorized",
    "publication_blockers",
}
EXPECTED_QUALIFICATION = {
    "pull_request": 178,
    "exact_head": "fc3c842205bb7ea46ef402826da865717a5a323e",
    "merge_commit": "914116cd531ded905a45547bee45c4bdefeeff43",
    "ci_run": 35431235301,
    "packageability_run": 35431235305,
    "baseline_integrity_run": 35431235300,
}
EXPECTED_BLOCKERS = [
    "crates_io_name_availability_must_be_rechecked_at_release_time",
    "full_dependency_order_registry_publish_not_executed",
    "clean_registry_downstream_install_not_yet_possible_without_first_publish",
    "non_leaf_package_archives_not_buildable_or_inspectable_until_dependency_order_publish",
]
REGISTRY_VISIBLE_PACKAGES = {
    "memorithm-elastic-core",
    "memorithm-elastic-macros",
    "memorithm-elastic-eir",
    "memorithm-elastic-adapters",
    "memorithm-elastic-runtime",
    "memorithm-elastic-kv",
    "memorithm-elastic",
}


def fail(message: str) -> None:
    raise SystemExit(f"internal-baseline: {message}")


def git(*args: str, binary: bool = False):
    return subprocess.check_output(
        ["git", *args], cwd=ROOT, text=not binary
    ).strip() if not binary else subprocess.check_output(["git", *args], cwd=ROOT)


def load_json(path: Path, label: str) -> dict[str, object]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot decode {label}: {error}")
    if not isinstance(data, dict):
        fail(f"{label} must be a JSON object")
    return data


def git_payload_digest(commit: str) -> tuple[str, int]:
    raw = subprocess.check_output(["git", "ls-tree", "-r", "-z", commit], cwd=ROOT)
    digest = hashlib.sha256()
    count = 0
    for record in raw.split(b"\0"):
        if not record:
            continue
        metadata, path_bytes = record.split(b"\t", 1)
        mode, object_type, object_id = metadata.decode("ascii").split()
        path = path_bytes.decode("utf-8")
        if path in EXCLUDED_PAYLOAD_PATHS:
            continue
        if object_type != "blob":
            fail(f"baseline payload contains non-blob tracked object {path!r}")
        content = subprocess.check_output(["git", "cat-file", "blob", object_id], cwd=ROOT)
        content_digest = hashlib.sha256(content).hexdigest()
        digest.update(mode.encode("ascii"))
        digest.update(b"\0")
        digest.update(path_bytes)
        digest.update(b"\0")
        digest.update(content_digest.encode("ascii"))
        digest.update(b"\0")
        count += 1
    return digest.hexdigest(), count


def validate_baseline(data: dict[str, object]) -> tuple[str, int]:
    if set(data) != EXPECTED_TOP_LEVEL_KEYS:
        fail("baseline manifest has unknown or missing top-level fields")
    if data["schema"] != 2 or data["scope"] != "elasticxxx-qualified-internal-baseline-v1":
        fail("unsupported internal baseline schema/scope")
    if data["release_version"] != "0.1.0" or data["release_line"] != "0.1.x":
        fail("unexpected retained baseline version/line")
    if data["msrv"] != "1.89":
        fail("retained baseline MSRV drifted")
    if data["payload_digest_algorithm"] != "sha256-git-mode-path-content-v1":
        fail("unsupported retained payload digest algorithm")
    if data["payload_exclusions"] != sorted(EXCLUDED_PAYLOAD_PATHS):
        fail("retained payload exclusions drifted")
    if data["baseline_qualification"] != EXPECTED_QUALIFICATION:
        fail("baseline exact-head qualification evidence drifted")
    if data["publication_suspended"] is not True:
        fail("publication must remain explicitly suspended")
    for field in (
        "registry_publication_authorized",
        "registry_mutation_authorized",
        "network_registry_queries_authorized",
    ):
        if data[field] is not False:
            fail(f"{field} must remain false")
    if data["publication_blockers"] != EXPECTED_BLOCKERS:
        fail("publication blocker set drifted")

    commit = data["baseline_source_commit"]
    tree = data["baseline_source_tree"]
    if not isinstance(commit, str) or not HEX40.fullmatch(commit):
        fail("invalid baseline source commit")
    if not isinstance(tree, str) or not HEX40.fullmatch(tree):
        fail("invalid baseline source tree")
    if commit != EXPECTED_QUALIFICATION["exact_head"]:
        fail("baseline source commit is not the qualified exact head")
    try:
        subprocess.run(
            ["git", "cat-file", "-e", f"{commit}^{{commit}}"],
            cwd=ROOT,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except subprocess.CalledProcessError:
        fail("qualified baseline commit is unavailable; checkout must retain repository history")
    actual_tree = git("rev-parse", f"{commit}^{{tree}}")
    if actual_tree != tree:
        fail(f"baseline tree mismatch: recorded={tree} actual={actual_tree}")
    digest, count = git_payload_digest(commit)
    if digest != data["payload_sha256"]:
        fail(
            "qualified baseline payload digest mismatch; "
            f"recorded={data['payload_sha256']} actual={digest}"
        )
    return digest, count


def validate_current_nonpublication(data: dict[str, object]) -> None:
    productization = load_json(PRODUCTIZATION, "productization manifest")
    if productization.get("registry_publication_authorized") is not False:
        fail("current productization manifest unexpectedly authorizes publication")
    if set(productization.get("publication_blockers", [])) != set(EXPECTED_BLOCKERS):
        fail("current productization blocker set drifted")

    root = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    if root["workspace"]["package"]["rust-version"] != data["msrv"]:
        fail("current workspace MSRV drifted from retained baseline")
    metadata = json.loads(
        subprocess.check_output(
            [
                "cargo",
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--locked",
                "--offline",
            ],
            cwd=ROOT,
            text=True,
        )
    )
    workspace_ids = set(metadata["workspace_members"])
    packages = {
        package["name"]: package
        for package in metadata["packages"]
        if package["id"] in workspace_ids
    }
    missing = REGISTRY_VISIBLE_PACKAGES - packages.keys()
    if missing:
        fail(f"current registry-visible package set is incomplete: {sorted(missing)!r}")
    for package_name in sorted(REGISTRY_VISIBLE_PACKAGES):
        package = packages[package_name]
        if package.get("publish") != []:
            fail(f"current package {package_name} must remain publish=false")
        if package["rust_version"] != data["msrv"]:
            fail(f"current package {package_name} MSRV drifted")


def validate_clean_checkout(require_clean: bool) -> None:
    if not require_clean:
        return
    if git("status", "--porcelain=v1", "--untracked-files=all"):
        fail("integrity check requires a clean current checkout")


def receipt(data: dict[str, object], payload: str, count: int) -> dict[str, object]:
    return {
        "schema": 2,
        "scope": "elasticxxx-qualified-internal-baseline-receipt-v1",
        "baseline_source_commit": data["baseline_source_commit"],
        "baseline_source_tree": data["baseline_source_tree"],
        "baseline_payload_sha256": payload,
        "baseline_tracked_payload_files": count,
        "current_head": git("rev-parse", "HEAD"),
        "current_tree": git("rev-parse", "HEAD^{tree}"),
        "publication_suspended": True,
        "registry_publication_authorized": False,
        "registry_mutation_authorized": False,
        "network_registry_queries_performed": False,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--print-payload-digest", action="store_true")
    parser.add_argument("--write-receipt", type=Path)
    parser.add_argument("--require-clean", action="store_true")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    data = load_json(BASELINE, "internal baseline manifest")
    payload, count = validate_baseline(data)
    if args.print_payload_digest:
        print(json.dumps({"payload_sha256": payload, "tracked_payload_files": count}, sort_keys=True))
        return
    validate_current_nonpublication(data)
    validate_clean_checkout(args.require_clean)

    result = receipt(data, payload, count)
    if args.write_receipt is not None:
        output = args.write_receipt
        if not output.is_absolute():
            output = ROOT / output
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(result, sort_keys=True))
    print("internal-baseline: qualified 0.1.0 baseline intact; publication remains suspended")


if __name__ == "__main__":
    main()
