#!/usr/bin/env bash
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"

if [[ -n "$(git status --porcelain)" ]]; then
  echo "BE13 build-attestation regression test requires a clean worktree" >&2
  exit 2
fi

TMP=$(mktemp -d)
TAG="refs/tags/be13-build-context-test-$$"
NESTED_ROOT="$TMP/parent/repo"
NESTED_ADDED=0

cleanup() {
  git tag -d "${TAG#refs/tags/}" >/dev/null 2>&1 || true
  if [[ "$NESTED_ADDED" == 1 ]]; then
    git worktree remove --force "$NESTED_ROOT" >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT INT TERM

git tag "${TAG#refs/tags/}" HEAD

expect_reject_in_root() {
  local root=$1
  local name=$2
  local expected=$3
  shift 3
  local out="$TMP/$name"
  local log="$TMP/$name.log"
  if (cd "$root" && "$@" ./scripts/collect-be13-portable-evidence.sh "$out") >"$log" 2>&1; then
    echo "$name unexpectedly succeeded" >&2
    cat "$log" >&2
    exit 1
  fi
  if ! grep -F "$expected" "$log" >/dev/null; then
    echo "$name failed for the wrong reason; expected: $expected" >&2
    cat "$log" >&2
    exit 1
  fi
}

expect_reject_in_root "$ROOT" missing-source-ref \
  'all BE13 v2 attested evidence requires BE13_SOURCE_REF=refs/tags/...' \
  env -u BE13_SOURCE_REF -u BE13_CODEGEN_PROFILE_EXPECTED -u BE13_REQUIRE_QUALIFIED

expect_reject_in_root "$ROOT" cargo-build-rustflags \
  'effective codegen profile custom does not match expected portable' \
  env BE13_SOURCE_REF="$TAG" BE13_REQUIRE_QUALIFIED=1 \
  BE13_CODEGEN_PROFILE_EXPECTED=portable CARGO_BUILD_RUSTFLAGS='-C target-cpu=native'

expect_reject_in_root "$ROOT" bench-profile-override \
  'effective codegen profile custom does not match expected portable' \
  env BE13_SOURCE_REF="$TAG" BE13_REQUIRE_QUALIFIED=1 \
  BE13_CODEGEN_PROFILE_EXPECTED=portable CARGO_PROFILE_BENCH_OPT_LEVEL=0

mkdir -p "$TMP/parent"
git worktree add --detach "$NESTED_ROOT" HEAD >/dev/null
NESTED_ADDED=1
mkdir -p "$TMP/parent/.cargo"
printf '%s\n' '[build]' "rustflags = ['--cfg', 'be13_parent_config_probe']" \
  >"$TMP/parent/.cargo/config.toml"
expect_reject_in_root "$NESTED_ROOT" parent-cargo-config \
  'effective codegen profile custom does not match expected portable' \
  env BE13_SOURCE_REF="$TAG" BE13_REQUIRE_QUALIFIED=1 BE13_CODEGEN_PROFILE_EXPECTED=portable

git worktree remove --force "$NESTED_ROOT" >/dev/null
NESTED_ADDED=0

expect_reject_in_root "$ROOT" missing-qualified-profile \
  'qualified BE13 evidence requires BE13_CODEGEN_PROFILE_EXPECTED=portable|native' \
  env -u BE13_CODEGEN_PROFILE_EXPECTED BE13_SOURCE_REF="$TAG" BE13_REQUIRE_QUALIFIED=1

echo 'BE13 build-attestation negative regression checks passed'
