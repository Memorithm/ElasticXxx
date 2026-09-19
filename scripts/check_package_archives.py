#!/usr/bin/env python3
"""Fail-closed inspection of BE15f first-publish leaf crate archives."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path, PurePosixPath
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.1.0"
MSRV = "1.89"
REPOSITORY = "https://github.com/Memorithm/ElasticXxx"
LEAVES = ("elastic-core", "elastic-macros")
ALLOWLIST = ROOT / "docs/release/PACKAGE-FILES-V1.json"
MAX_ALLOWLIST_BYTES = 64 * 1024
MAX_ARCHIVE_BYTES = 16 * 1024 * 1024
MAX_UNPACKED_BYTES = 32 * 1024 * 1024
MAX_MEMBERS = 4096
MAX_MEMBER_NAME_BYTES = 512


def fail(message: str) -> None:
    raise SystemExit(f"package-archive: {message}")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def safe_member_name(name: str, prefix: str) -> str:
    if not name.startswith(prefix + "/"):
        fail(f"archive member escapes package prefix: {name!r}")
    if "\\" in name or "\x00" in name or len(name.encode()) > MAX_MEMBER_NAME_BYTES:
        fail(f"unsafe archive member name: {name!r}")
    rel = name[len(prefix) + 1 :]
    path = PurePosixPath(rel)
    if not rel or path.is_absolute() or any(part in ("", ".", "..") for part in path.parts):
        fail(f"unsafe archive member path: {name!r}")
    return rel


def cargo_file_set(package: str, prefix: str) -> set[str]:
    output = subprocess.check_output(
        ["cargo", "package", "-p", package, "--allow-dirty", "--list"],
        cwd=ROOT,
        text=True,
    )
    names = {line.strip() for line in output.splitlines() if line.strip()}
    if not names:
        fail(f"cargo package --list returned no files for {package}")
    return {f"{prefix}/{name}" for name in names}


def reviewed_file_set(package: str, prefix: str) -> set[str]:
    raw = ALLOWLIST.read_bytes()
    if len(raw) > MAX_ALLOWLIST_BYTES:
        fail("reviewed package-file allowlist exceeds 64 KiB")
    data = json.loads(raw)
    if not isinstance(data, dict) or set(data) != {"schema", "scope", "packages"}:
        fail("reviewed package-file allowlist has unknown or missing fields")
    if data["schema"] != 1 or data["scope"] != "elasticxxx-first-publish-leaf-files-v1":
        fail("unsupported package-file allowlist schema/scope")
    packages = data["packages"]
    if not isinstance(packages, dict) or set(packages) != set(LEAVES):
        fail("reviewed package-file allowlist must cover exactly the first-publish leaves")
    names = packages.get(package)
    if not isinstance(names, list) or not names or any(not isinstance(name, str) for name in names):
        fail(f"invalid reviewed package-file allowlist for {package}")
    if len(names) != len(set(names)) or len(names) > MAX_MEMBERS:
        fail(f"duplicate or oversized reviewed package-file allowlist for {package}")
    prefixed = {f"{prefix}/{name}" for name in names}
    for name in prefixed:
        safe_member_name(name, prefix)
    return prefixed


def read_member(archive: tarfile.TarFile, member: str, max_bytes: int = 1024 * 1024) -> bytes:
    info = archive.getmember(member)
    if not info.isfile() or info.size > max_bytes:
        fail(f"invalid or oversized required archive member: {member}")
    handle = archive.extractfile(info)
    if handle is None:
        fail(f"cannot read required archive member: {member}")
    value = handle.read(max_bytes + 1)
    if len(value) > max_bytes:
        fail(f"oversized required archive member: {member}")
    return value


def validate_manifest(package: str, prefix: str, archive: tarfile.TarFile) -> None:
    manifest = tomllib.loads(read_member(archive, f"{prefix}/Cargo.toml").decode("utf-8"))
    metadata = manifest.get("package", {})
    expected = {
        "name": package,
        "version": VERSION,
        "rust-version": MSRV,
        "repository": REPOSITORY,
        "license-file": "LICENSE.md",
        "publish": False,
    }
    for key, value in expected.items():
        if metadata.get(key) != value:
            fail(f"{package} packaged Cargo.toml {key!r} drifted: {metadata.get(key)!r}")
    if not isinstance(metadata.get("description"), str) or not metadata["description"].strip():
        fail(f"{package} packaged Cargo.toml needs a non-empty description")


def validate_crate_level_rustdoc(package: str, archived_lib: bytes) -> None:
    source_lib = ROOT / "crates" / package / "src/lib.rs"
    if source_lib.read_bytes() != archived_lib:
        fail(f"{package} archived src/lib.rs differs from the exact source tree")
    try:
        subprocess.run(
            [
                "cargo",
                "rustdoc",
                "-p",
                package,
                "--lib",
                "--quiet",
                "--",
                "-D",
                "rustdoc::missing-crate-level-docs",
            ],
            cwd=ROOT,
            check=True,
        )
    except subprocess.CalledProcessError:
        fail(f"{package} failed syntax-aware crate-level rustdoc validation")


def validate_archive(package: str) -> None:
    prefix = f"{package}-{VERSION}"
    path = ROOT / "target/package" / f"{prefix}.crate"
    if not path.is_file() or path.stat().st_size > MAX_ARCHIVE_BYTES:
        fail(f"missing or oversized archive: {path.relative_to(ROOT)}")
    reviewed_files = reviewed_file_set(package, prefix)
    cargo_files = cargo_file_set(package, prefix)
    if cargo_files != reviewed_files:
        missing = sorted(reviewed_files - cargo_files)
        extra = sorted(cargo_files - reviewed_files)
        fail(
            f"{package} Cargo package file set drifted from reviewed allowlist; "
            f"missing={missing!r} extra={extra!r}"
        )
    with tarfile.open(path, mode="r:gz") as archive:
        members = archive.getmembers()
        if len(members) > MAX_MEMBERS:
            fail(f"{package} archive has too many members")
        names = [member.name for member in members]
        if len(names) != len(set(names)):
            fail(f"{package} archive contains duplicate member names")
        if any(not member.isfile() for member in members):
            fail(f"{package} archive contains a non-regular member")
        for name in names:
            safe_member_name(name, prefix)
        if set(names) != reviewed_files:
            missing = sorted(reviewed_files - set(names))
            extra = sorted(set(names) - reviewed_files)
            fail(
                f"{package} archive file set differs from reviewed allowlist; "
                f"missing={missing!r} extra={extra!r}"
            )
        if sum(member.size for member in members) > MAX_UNPACKED_BYTES:
            fail(f"{package} archive expands beyond the bounded size")
        validate_manifest(package, prefix, archive)
        license_bytes = read_member(archive, f"{prefix}/LICENSE.md")
        if sha256(license_bytes) != sha256((ROOT / "LICENSE.md").read_bytes()):
            fail(f"{package} archive license differs from repository LICENSE.md")
        lib = read_member(archive, f"{prefix}/src/lib.rs")
        validate_crate_level_rustdoc(package, lib)
        vcs = json.loads(read_member(archive, f"{prefix}/.cargo_vcs_info.json").decode("utf-8"))
        head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        expected_vcs_path = f"crates/{package}"
        if vcs.get("git", {}).get("sha1") != head or vcs.get("path_in_vcs") != expected_vcs_path:
            fail(f"{package} archive VCS provenance does not match exact repository head")


def main() -> None:
    for package in LEAVES:
        validate_archive(package)
    print("package-archive: leaf archives match Cargo file sets and release metadata")
    print("package-archive: publication remains unauthorized")


if __name__ == "__main__":
    main()
