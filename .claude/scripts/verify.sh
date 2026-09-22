#!/usr/bin/env bash
# Single verification entry point. Runs at explicit points only (implementer end, reviewer,
# before a PR) — never from a hook. The reviewer reruns it and never approves on red.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

echo "== fmt =="
cargo fmt --all --check

echo "== clippy =="
cargo clippy --workspace --all-targets --quiet -- -D warnings

echo "== test =="
if command -v cargo-nextest >/dev/null 2>&1; then
  cargo nextest run --workspace --no-fail-fast
  cargo test --workspace --doc --quiet # nextest does not run doctests
else
  cargo test --workspace --quiet
fi

echo "== spike leakage =="
# A workspace crate path-depending on a spike compiles fine; only this grep catches it.
if grep -rn --include=Cargo.toml -E 'path *= *"[^"]*features/' crates; then
  echo "FAIL: a workspace crate depends on a spike" >&2
  exit 1
fi

echo "== doc budgets =="
budget() { # path -> max words
  case "$1" in
    docs/constitution.md) echo 500 ;;
    docs/architecture.md) echo 500 ;;
    features/*/spec.md) echo 800 ;;
    features/*/spike/findings.md) echo 400 ;;
    *) echo 0 ;;
  esac
}
fail=0 checked=0
for f in docs/constitution.md docs/architecture.md features/*/spec.md features/*/spike/findings.md; do
  [[ -f $f ]] || continue
  checked=$((checked + 1))
  max=$(budget "$f")
  words=$(wc -w <"$f")
  if ((words > max)); then
    echo "OVER BUDGET: $f ($words > $max words). Cut $((words - max)) — criteria and evidence stay, prose goes." >&2
    fail=1
  fi
done
# A check that evaluated nothing is a broken check, not a clean bill of health.
((checked > 0)) || { echo "FAIL: doc budget checked 0 files" >&2; exit 1; }
((fail == 0)) || exit 1

echo "verify: ALL GREEN"
