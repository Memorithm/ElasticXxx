#!/usr/bin/env bash
# Dry-run crates.io publish rehearsal for ElasticXxx.
# NEVER calls cargo publish, NEVER uses a registry token, NEVER mutates crates.io.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

UA="${CRATES_IO_USER_AGENT:-Memorithm-release-audit/1.0 contact@checkupauto.fr}"
API="https://crates.io/api/v1/crates"

REGISTRY_PACKAGES=(
  memorithm-elastic-core
  memorithm-elastic-language-syntax
  memorithm-elastic-macros
  memorithm-elastic-eir
  memorithm-elastic-adapters
  memorithm-elastic-runtime
  memorithm-elastic-kv
  memorithm-elastic
)

# Dependency-order publish sequence (authorized releases only; this script never publishes).
PUBLISH_ORDER=(
  memorithm-elastic-core
  memorithm-elastic-language-syntax
  memorithm-elastic-macros
  memorithm-elastic-eir
  memorithm-elastic-adapters
  memorithm-elastic-runtime
  memorithm-elastic-kv
  memorithm-elastic
)

LEAF_PACKAGEABLE_TODAY=(
  memorithm-elastic-core
  memorithm-elastic-language-syntax
)

fail() {
  echo "rehearse-crates-io: ERROR: $*" >&2
  exit 1
}

echo "rehearse-crates-io: starting dry-run rehearsal (no cargo publish, no token)"
echo "rehearse-crates-io: repository=$ROOT"

# 1) Every registry-visible package must still have publish = false.
echo "rehearse-crates-io: verifying publish=false on public topology"
metadata="$(cargo metadata --format-version 1 --no-deps)"
python3 - "$metadata" <<'PY'
import json, sys
wanted = {
    "memorithm-elastic-core",
    "memorithm-elastic-language-syntax",
    "memorithm-elastic-macros",
    "memorithm-elastic-eir",
    "memorithm-elastic-adapters",
    "memorithm-elastic-runtime",
    "memorithm-elastic-kv",
    "memorithm-elastic",
}
meta = json.loads(sys.argv[1])
members = set(meta["workspace_members"])
found = {}
for pkg in meta["packages"]:
    if pkg["id"] not in members:
        continue
    if pkg["name"] in wanted:
        found[pkg["name"]] = pkg
missing = sorted(wanted - set(found))
if missing:
    raise SystemExit(f"missing registry packages in workspace metadata: {missing}")
for name, pkg in sorted(found.items()):
    # Cargo encodes publish=false as an empty allow-list.
    if pkg.get("publish") != []:
        raise SystemExit(
            f"{name} publish allow-list is {pkg.get('publish')!r}; "
            "expected publish=false (empty). Do not flip publish without authority."
        )
    print(f"  ok publish=false: {name}")
PY

# 2) Point-in-time crates.io name recheck (read-only GET).
echo "rehearse-crates-io: rechecking selected names on crates.io (User-Agent required)"
recheck_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "  observed_at_utc=$recheck_at"
for name in "${REGISTRY_PACKAGES[@]}"; do
  code="$(curl -sS -o /tmp/"rehearse_${name}.json" -w '%{http_code}' -A "$UA" "$API/$name" || true)"
  case "$code" in
    404)
      echo "  $name -> HTTP 404 (absent at this instant; not a reservation)"
      ;;
    200)
      echo "  $name -> HTTP 200 (EXISTS — release-time conflict; fail closed)"
      fail "$name is already present on crates.io; do not publish under a colliding name"
      ;;
    *)
      fail "unexpected HTTP $code for $name (need network + identifying User-Agent)"
      ;;
  esac
done

# 3) Package file-set listing for the full facade chain; real archives for leaves only.
echo "rehearse-crates-io: cargo package --list for full public chain"
for name in "${PUBLISH_ORDER[@]}"; do
  cargo package -p "$name" --list --allow-dirty >/tmp/"rehearse_list_${name}.txt"
  lines="$(wc -l < /tmp/"rehearse_list_${name}.txt" | tr -d ' ')"
  echo "  $name --list entries=$lines"
done

echo "rehearse-crates-io: building leaf archives with cargo package --no-verify"
for name in "${LEAF_PACKAGEABLE_TODAY[@]}"; do
  cargo package -p "$name" --no-verify --allow-dirty
done

echo "rehearse-crates-io: inspecting leaf archives via check_package_archives.py"
python3 scripts/check_package_archives.py

# 4) Document authorized-sequence (informational only).
echo
echo "rehearse-crates-io: authorized dependency-order publish sequence (DO NOT RUN HERE):"
for i in "${!PUBLISH_ORDER[@]}"; do
  printf '  %d. cargo publish -p %s   # only after explicit release authority\n' "$((i + 1))" "${PUBLISH_ORDER[$i]}"
done
echo
echo "rehearse-crates-io: remaining P1 blockers before a real publish:"
echo "  - explicit user/release authority to set publish=true and run cargo publish"
echo "  - fresh release-time name recheck immediately before each upload"
echo "  - dependency-order registry publication of the eight packages"
echo "  - non-leaf archive build/inspect after each dependency exists on the registry"
echo "  - clean downstream install of memorithm-elastic without workspace paths"
echo
echo "rehearse-crates-io: SUCCESS (dry-run only; publication remains unauthorized)"
