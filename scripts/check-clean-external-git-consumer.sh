#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_REPOSITORY="${ELASTIC_SOURCE_REPOSITORY:-https://github.com/Memorithm/ElasticXxx.git}"
SOURCE_REV="${ELASTIC_SOURCE_REV:-}"

if [[ ! "$SOURCE_REV" =~ ^[0-9a-f]{40}$ ]]; then
  echo "clean-external-git-consumer: ELASTIC_SOURCE_REV must be one exact lowercase 40-hex commit" >&2
  exit 1
fi

fixture="$ROOT/fixtures/external-adapter-v1/src/lib.rs"
if [[ ! -f "$fixture" ]]; then
  echo "clean-external-git-consumer: missing standalone adapter fixture" >&2
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/src"
cp "$fixture" "$tmp/src/lib.rs"

cat > "$tmp/Cargo.toml" <<EOF
[package]
name = "elastic-external-adapter-v1-git-consumer"
version = "0.0.0"
edition = "2021"
rust-version = "1.89"
publish = false

[dependencies]
elastic = { package = "memorithm-elastic", git = "$SOURCE_REPOSITORY", rev = "$SOURCE_REV" }
EOF

cargo +1.89.0 generate-lockfile --manifest-path "$tmp/Cargo.toml"
cargo +1.89.0 metadata   --manifest-path "$tmp/Cargo.toml"   --format-version 1   --locked > "$tmp/metadata.json"

python3 - "$tmp/metadata.json" "$SOURCE_REV" <<'PY'
import json
import sys

metadata_path, revision = sys.argv[1:]
metadata = json.load(open(metadata_path, encoding="utf-8"))

elastic_packages = [
    package
    for package in metadata["packages"]
    if package["name"].startswith("memorithm-elastic")
]
if not elastic_packages:
    raise SystemExit("clean external consumer resolved no Memorithm Elastic packages")

for package in elastic_packages:
    source = package.get("source")
    if source is None:
        raise SystemExit(
            f"{package['name']} unexpectedly resolved as a path/workspace package"
        )
    if f"?rev={revision}#{revision}" not in source:
        raise SystemExit(
            f"{package['name']} resolved from unexpected source {source!r}"
        )

facades = [package for package in elastic_packages if package["name"] == "memorithm-elastic"]
if len(facades) != 1:
    raise SystemExit(
        f"expected exactly one memorithm-elastic facade package, got {len(facades)}"
    )

root_package = next(
    package
    for package in metadata["packages"]
    if package["name"] == "elastic-external-adapter-v1-git-consumer"
)
dependencies = root_package.get("dependencies", [])
if len(dependencies) != 1:
    raise SystemExit(
        f"clean external project must declare exactly one dependency, got {len(dependencies)}"
    )
dependency = dependencies[0]
if dependency.get("name") != "memorithm-elastic" or dependency.get("rename") != "elastic":
    raise SystemExit(
        "clean external project must depend only on memorithm-elastic under the elastic alias"
    )

print(
    "clean-external-git-consumer: exact git source resolved; "
    f"{len(elastic_packages)} Memorithm Elastic packages share revision {revision}"
)
PY

cargo +1.89.0 test   --manifest-path "$tmp/Cargo.toml"   --locked

cargo +1.89.0 clippy   --manifest-path "$tmp/Cargo.toml"   --all-targets   --locked   -- -D warnings

RUSTDOCFLAGS="-D warnings" cargo +1.89.0 doc   --manifest-path "$tmp/Cargo.toml"   --locked   --no-deps

echo "CLEAN_EXTERNAL_GIT_CONSUMER_QUALIFIED revision=$SOURCE_REV"
