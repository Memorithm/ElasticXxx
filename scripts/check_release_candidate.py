#!/usr/bin/env python3
"""Fail-closed BE15f release-candidate freeze and prepublication gate.

This checker never mutates a registry. It binds one reviewed release payload to a
stable path/content digest and emits a receipt that names the exact Git commit on
which the check ran. Publication remains independently unauthorized.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
CANDIDATE = ROOT / "docs/release/RELEASE-CANDIDATE-V1.json"
PRODUCTIZATION = ROOT / "docs/release/PRODUCTIZATION-V1.json"
EXCLUDED_PAYLOAD_PATHS = {"docs/release/RELEASE-CANDIDATE-V1.json"}
EXPECTED_TOP_LEVEL_KEYS = {
    "schema",
    "scope",
    "release_version",
    "release_line",
    "msrv",
    "payload_digest_algorithm",
    "payload_sha256",
    "payload_exclusions",
    "registry_publication_authorized",
    "registry_mutation_authorized",
    "network_registry_queries_authorized",
    "required_exact_commit_checks",
    "publication_blockers",
}
EXPECTED_CHECKS = ["ci", "packageability", "release-candidate-prepublication"]
EXPECTED_BLOCKERS = [
    "crates_io_name_availability_must_be_rechecked_at_release_time",
    "full_dependency_order_registry_publish_not_executed",
    "clean_registry_downstream_install_not_yet_possible_without_first_publish",
    "non_leaf_package_archives_not_buildable_or_inspectable_until_dependency_order_publish",
    "exact_release_commit_required_ci_and_packageability_not_yet_successful",
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
    raise SystemExit(f"release-candidate: {message}")


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def tracked_paths() -> list[str]:
    raw = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT)
    paths = [item.decode("utf-8") for item in raw.split(b"\0") if item]
    return sorted(path for path in paths if path not in EXCLUDED_PAYLOAD_PATHS)


def working_tree_mode_and_bytes(path: Path, rel: str) -> tuple[str, bytes]:
    try:
        info = path.lstat()
    except OSError as error:
        fail(f"cannot stat tracked payload path {rel}: {error}")
    if stat.S_ISLNK(info.st_mode):
        try:
            target = os.readlink(path)
        except OSError as error:
            fail(f"cannot read tracked symlink {rel}: {error}")
        return "120000", os.fsencode(target)
    if not stat.S_ISREG(info.st_mode):
        fail(f"tracked payload path is neither a regular file nor symlink: {rel}")
    mode = "100755" if info.st_mode & stat.S_IXUSR else "100644"
    return mode, path.read_bytes()


def payload_digest() -> tuple[str, int]:
    digest = hashlib.sha256()
    count = 0
    for rel in tracked_paths():
        mode, content = working_tree_mode_and_bytes(ROOT / rel, rel)
        content_digest = hashlib.sha256(content).hexdigest()
        digest.update(mode.encode("ascii"))
        digest.update(b"\0")
        digest.update(rel.encode("utf-8"))
        digest.update(b"\0")
        digest.update(content_digest.encode("ascii"))
        digest.update(b"\0")
        count += 1
    return digest.hexdigest(), count


def load_json(path: Path, label: str) -> dict[str, object]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot decode {label}: {error}")
    if not isinstance(data, dict):
        fail(f"{label} must be a JSON object")
    return data


def validate_candidate(data: dict[str, object]) -> tuple[str, int]:
    if set(data) != EXPECTED_TOP_LEVEL_KEYS:
        fail("release-candidate manifest has unknown or missing top-level fields")
    if data["schema"] != 1 or data["scope"] != "elasticxxx-release-candidate-freeze-v1":
        fail("unsupported release-candidate schema/scope")
    if data["release_version"] != "0.1.0" or data["release_line"] != "0.1.x":
        fail("unexpected release candidate version/line")
    if data["msrv"] != "1.89":
        fail("release candidate MSRV drifted")
    if data["payload_digest_algorithm"] != "sha256-git-mode-path-content-v1":
        fail("unsupported release payload digest algorithm")
    if data["payload_exclusions"] != sorted(EXCLUDED_PAYLOAD_PATHS):
        fail("release payload exclusions drifted")
    for field in (
        "registry_publication_authorized",
        "registry_mutation_authorized",
        "network_registry_queries_authorized",
    ):
        if data[field] is not False:
            fail(f"{field} must remain false in the prepublication candidate")
    if data["required_exact_commit_checks"] != EXPECTED_CHECKS:
        fail("required exact-commit check set drifted")
    if data["publication_blockers"] != EXPECTED_BLOCKERS:
        fail("release-candidate publication blockers drifted")

    current_digest, count = payload_digest()
    if data["payload_sha256"] != current_digest:
        fail(
            "release payload drifted from the frozen candidate digest; "
            f"recorded={data['payload_sha256']} current={current_digest}"
        )
    return current_digest, count


def validate_productization(candidate: dict[str, object]) -> None:
    productization = load_json(PRODUCTIZATION, "productization manifest")
    if productization.get("workspace_version") != candidate["release_version"]:
        fail("candidate version disagrees with productization manifest")
    if productization.get("release_line") != candidate["release_line"]:
        fail("candidate release line disagrees with productization manifest")
    if productization.get("msrv") != candidate["msrv"]:
        fail("candidate MSRV disagrees with productization manifest")
    if productization.get("registry_publication_authorized") is not False:
        fail("productization manifest unexpectedly authorizes registry publication")
    blockers = productization.get("publication_blockers")
    if not isinstance(blockers, list):
        fail("productization publication blockers are malformed")
    if set(blockers) != set(EXPECTED_BLOCKERS):
        fail("productization blocker set drifted from release-candidate expectations")


def validate_workspace(candidate: dict[str, object]) -> None:
    root = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    package = root["workspace"]["package"]
    if package["rust-version"] != candidate["msrv"]:
        fail("workspace MSRV disagrees with candidate")

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
        p["name"]: p for p in metadata["packages"] if p["id"] in workspace_ids
    }
    missing = REGISTRY_VISIBLE_PACKAGES - packages.keys()
    if missing:
        fail(f"registry-visible package set is incomplete: {sorted(missing)!r}")
    for package_name in sorted(REGISTRY_VISIBLE_PACKAGES):
        package = packages[package_name]
        if package["version"] != candidate["release_version"]:
            fail(f"{package_name} version disagrees with candidate")
        if package["rust_version"] != candidate["msrv"]:
            fail(f"{package_name} MSRV disagrees with candidate")
        # Cargo metadata encodes `publish = false` as an empty allow-list.
        if package.get("publish") != []:
            fail(f"{package_name} must remain publish=false")


def validate_clean_checkout(require_clean: bool) -> None:
    if not require_clean:
        return
    status = git("status", "--porcelain=v1", "--untracked-files=all")
    if status:
        fail("exact-candidate check requires a clean Git checkout")


def receipt(payload: str, tracked_file_count: int) -> dict[str, object]:
    head = git("rev-parse", "HEAD")
    if len(head) != 40:
        fail("cannot resolve exact candidate commit")
    tree = git("rev-parse", "HEAD^{tree}")
    return {
        "schema": 1,
        "scope": "elasticxxx-release-candidate-receipt-v1",
        "source_commit": head,
        "source_tree": tree,
        "release_version": "0.1.0",
        "payload_sha256": payload,
        "tracked_payload_files": tracked_file_count,
        "registry_publication_authorized": False,
        "registry_mutation_authorized": False,
        "network_registry_queries_performed": False,
        "required_exact_commit_checks": EXPECTED_CHECKS,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--print-payload-digest", action="store_true")
    parser.add_argument("--write-receipt", type=Path)
    parser.add_argument("--require-clean", action="store_true")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.print_payload_digest:
        digest, count = payload_digest()
        print(json.dumps({"payload_sha256": digest, "tracked_payload_files": count}, sort_keys=True))
        return

    candidate = load_json(CANDIDATE, "release-candidate manifest")
    payload, count = validate_candidate(candidate)
    validate_productization(candidate)
    validate_workspace(candidate)
    validate_clean_checkout(args.require_clean)

    # Reuse the stronger pre-release productization gate rather than creating a
    # second interpretation of package topology/licensing/reviewed blockers.
    offline_env = os.environ.copy()
    offline_env["CARGO_NET_OFFLINE"] = "true"
    subprocess.run(
        [sys.executable, str(ROOT / "scripts/check_release_productization.py")],
        cwd=ROOT,
        env=offline_env,
        check=True,
    )

    result = receipt(payload, count)
    if args.write_receipt is not None:
        output = args.write_receipt
        if not output.is_absolute():
            output = ROOT / output
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(result, sort_keys=True))
    print("release-candidate: frozen payload valid; registry publication and mutation remain unauthorized")


if __name__ == "__main__":
    main()
