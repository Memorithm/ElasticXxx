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
LEAVES = (
    ("memorithm-elastic-core", "elastic-core", "elastic_core"),
    ("memorithm-elastic-macros", "elastic-macros", "elastic_macros"),
)
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


def validate_manifest(package: str, crate_name: str, prefix: str, archive: tarfile.TarFile) -> None:
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
    if manifest.get("lib", {}).get("name") != crate_name:
        fail(f"{package} packaged Cargo.toml must preserve library crate name {crate_name!r}")


def validate_archive(package: str, source_dir: str, crate_name: str) -> None:
    prefix = f"{package}-{VERSION}"
    path = ROOT / "target/package" / f"{prefix}.crate"
    if not path.is_file() or path.stat().st_size > MAX_ARCHIVE_BYTES:
        fail(f"missing or oversized archive: {path.relative_to(ROOT)}")
    expected_files = cargo_file_set(package, prefix)
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
        if set(names) != expected_files:
            missing = sorted(expected_files - set(names))
            extra = sorted(set(names) - expected_files)
            fail(f"{package} archive file set differs from cargo --list; missing={missing!r} extra={extra!r}")
        if sum(member.size for member in members) > MAX_UNPACKED_BYTES:
            fail(f"{package} archive expands beyond the bounded size")
        validate_manifest(package, crate_name, prefix, archive)
        license_bytes = read_member(archive, f"{prefix}/LICENSE.md")
        if sha256(license_bytes) != sha256((ROOT / "LICENSE.md").read_bytes()):
            fail(f"{package} archive license differs from repository LICENSE.md")
        lib = read_member(archive, f"{prefix}/src/lib.rs")
        if b"//!" not in lib[:4096]:
            fail(f"{package} archive lacks crate-level rustdoc near src/lib.rs start")
        vcs = json.loads(read_member(archive, f"{prefix}/.cargo_vcs_info.json").decode("utf-8"))
        head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        expected_vcs_path = f"crates/{source_dir}"
        if vcs.get("git", {}).get("sha1") != head or vcs.get("path_in_vcs") != expected_vcs_path:
            fail(f"{package} archive VCS provenance does not match exact repository head")


def main() -> None:
    for package, source_dir, crate_name in LEAVES:
        validate_archive(package, source_dir, crate_name)
    print("package-archive: leaf archives match Cargo file sets and release metadata")
    print("package-archive: publication remains unauthorized")


if __name__ == "__main__":
    main()
