#!/usr/bin/env bash
# Mechanical drift checks between the docs site (docs/site) and the code (features/69/spec.md).
# Run by verify.sh. Each check is named after its criterion, fails naming the offending item,
# and fails when it evaluated nothing (a check that saw zero items is broken, not clean).
#   docs-check.sh             run every check against docs/site and crates/
#   docs-check.sh --selftest  prove each check fails on a mutated copy and passes on the real one
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SITE=docs/site
BINS=(hauz hauz-tui hauz-web)
declare -A PKG=([hauz]=hauz-cli [hauz-tui]=hauz-tui [hauz-web]=hauz-web)

fail() { echo "docs-check: $*" >&2; return 1; }

# AC1: the book builds; a missing mdbook or mdbook-mermaid is a failure, not a skip.
ac1_site_builds() { # site_dir
  command -v mdbook >/dev/null || fail "ac1: mdbook not installed (see docs/site/src/maintaining.md)" || return
  command -v mdbook-mermaid >/dev/null || fail "ac1: mdbook-mermaid not installed" || return
  local out
  out=$(mktemp -d)
  if ! mdbook build "$1" -d "$out" >/dev/null 2>&1; then
    rm -rf "$out"
    fail "ac1: mdbook build $1 failed (run it for the error)"
    return
  fi
  rm -rf "$out"
}

# AC2: every relative .md link (SUMMARY included) names an existing file. Anchors are ignored.
ac2_links_resolve() { # src_dir
  local src=$1 n=0 bad=0 page link target
  while IFS= read -r page; do
    while IFS= read -r link; do
      [[ -z $link || $link == *://* ]] && continue
      n=$((n + 1))
      target="$(dirname "$page")/${link%%#*}"
      [[ -f $target ]] || { echo "docs-check: ac2: $page links to missing $link" >&2; bad=1; }
    done < <(grep -oE '\]\([^)[:space:]]+\.md(#[^)]*)?\)' "$page" | sed -E 's/^\]\(//; s/\)$//')
  done < <(find "$src" -name '*.md')
  ((n > 0)) || fail "ac7: ac2 checked 0 links" || return
  ((bad == 0)) || return 1
}

# The usage text a binary prints for an unknown flag (stderr), which must exit 2.
usage_of() { # bin
  local bin=$1 out code=0
  out=$(cargo run -q -p "${PKG[$bin]}" --bin "$bin" -- --nope 2>&1 >/dev/null) || code=$?
  ((code == 2)) || { fail "ac3: $bin --nope exited $code, expected 2"; return; }
  printf '%s\n' "$out"
}

# AC3: the fenced block after `<!-- usage:<bin> -->` on the CLI reference equals the binary's usage.
ac3_usage_matches() { # src_dir usage_dir (one file per bin, from usage_of)
  local page=$1/reference/cli.md n=0 bad=0 bin documented
  for bin in "${BINS[@]}"; do
    documented=$(awk -v m="<!-- usage:$bin -->" '
      $0 == m { found = 1; next }
      found && /^```/ { if (inblock) exit; inblock = 1; next }
      inblock { print }' "$page")
    [[ -n $documented ]] || { echo "docs-check: ac3: no usage block for $bin in $page" >&2; bad=1; continue; }
    n=$((n + 1))
    [[ $documented == "$(cat "$2/$bin")" ]] ||
      { echo "docs-check: ac3: usage for $bin in $page differs from the binary's" >&2; bad=1; }
  done
  ((n > 0)) || fail "ac7: ac3 checked 0 binaries" || return
  ((bad == 0)) || return 1
}

# AC4: every upper-case string literal containing `_` in crates/*/src (an env var name) is on
# the env reference page as `NAME`.
ac4_env_documented() { # src_dir crates_dir
  local page=$1/setup/env.md n=0 bad=0 var
  while IFS= read -r var; do
    n=$((n + 1))
    grep -qF "\`$var\`" "$page" || { echo "docs-check: ac4: $var is read in code but missing from $page" >&2; bad=1; }
  done < <(grep -rhoE --include='*.rs' '"[A-Z][A-Z0-9]*_[A-Z0-9_]+"' "$2"/*/src 2>/dev/null | tr -d '"' | sort -u)
  ((n > 0)) || fail "ac7: ac4 checked 0 variables" || return
  ((bad == 0)) || return 1
}

# AC5: every server `.route("…")` path is on the API page and every web `#[route(M "…")]` path
# on the web page, as `path`.
ac5_routes_documented() { # src_dir crates_dir
  local n=0 bad=0 path
  while IFS= read -r path; do
    n=$((n + 1))
    grep -qF "\`$path\`" "$1/reference/api.md" || { echo "docs-check: ac5: route $path missing from reference/api.md" >&2; bad=1; }
  done < <(grep -rhoE --include='*.rs' '\.route\("[^"]+"' "$2"/server/src 2>/dev/null | sed -E 's/.*"([^"]+)"/\1/' | sort -u)
  while IFS= read -r path; do
    n=$((n + 1))
    grep -qF "\`$path\`" "$1/reference/web.md" || { echo "docs-check: ac5: route $path missing from reference/web.md" >&2; bad=1; }
  done < <(grep -rhoE --include='*.rs' '#\[route\([A-Z]+ "[^"]+"' "$2"/web/src 2>/dev/null | sed -E 's/.*"([^"]+)"/\1/' | sort -u)
  ((n > 0)) || fail "ac7: ac5 checked 0 routes" || return
  ((bad == 0)) || return 1
}

# AC6: every page but SUMMARY.md has a `*Verified against: `<sha|unaudited>`*` line.
ac6_pages_stamped() { # src_dir
  local n=0 bad=0 page
  while IFS= read -r page; do
    [[ $(basename "$page") == SUMMARY.md ]] && continue
    n=$((n + 1))
    grep -qE '^\*Verified against: `([0-9a-f]{7,40}|unaudited)`\*$' "$page" ||
      { echo "docs-check: ac6: $page has no Verified against line" >&2; bad=1; }
  done < <(find "$1" -name '*.md')
  ((n > 0)) || fail "ac7: ac6 checked 0 pages" || return
  ((bad == 0)) || return 1
}

collect_usage() { # out_dir
  local bin
  for bin in "${BINS[@]}"; do usage_of "$bin" >"$1/$bin" || return 1; done
}

run_all() { # site_dir crates_dir usage_dir
  local rc=0
  ac1_site_builds "$1" || rc=1
  ac2_links_resolve "$1/src" || rc=1
  ac3_usage_matches "$1/src" "$3" || rc=1
  ac4_env_documented "$1/src" "$2" || rc=1
  ac5_routes_documented "$1/src" "$2" || rc=1
  ac6_pages_stamped "$1/src" || rc=1
  return $rc
}

# Runs `check args…` and requires it to fail (stderr silenced); records a broken check otherwise.
expect_fail() { # label check args…
  local label=$1
  shift
  if "$@" 2>/dev/null; then
    echo "docs-check selftest: $label did NOT fail on a mutated copy" >&2
    st_bad=1
  fi
}

selftest() {
  local tmp usage
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' RETURN
  usage=$tmp/usage
  mkdir -p "$usage" "$tmp/empty/x/src"
  collect_usage "$usage"
  st_bad=0
  run_all "$SITE" crates "$usage" || { echo "docs-check selftest: real site fails" >&2; return 1; }

  fresh() { rm -rf "$tmp/site" && cp -r "$SITE" "$tmp/site" && rm -rf "$tmp/site/book"; }

  fresh; sed -i 's/command = "mdbook-mermaid"/command = "mdbook-no-such-preprocessor"/' "$tmp/site/book.toml"
  expect_fail ac1 ac1_site_builds "$tmp/site"

  fresh; echo '[x](no-such-page.md)' >>"$tmp/site/src/index.md"
  expect_fail ac2 ac2_links_resolve "$tmp/site/src"

  fresh; sed -i 's/usage: hauz-tui \[--db/usage: hauz-tui [--database/' "$tmp/site/src/reference/cli.md"
  expect_fail ac3 ac3_usage_matches "$tmp/site/src" "$usage"

  fresh; sed -i '/`HAUZ_API_TOKEN`/d' "$tmp/site/src/setup/env.md"
  expect_fail ac4 ac4_env_documented "$tmp/site/src" crates

  fresh; sed -i 's|`/v1/ingest/email`|`/v1/ingest`|g' "$tmp/site/src/reference/api.md"
  expect_fail ac5-server ac5_routes_documented "$tmp/site/src" crates
  fresh; sed -i 's|`/style.css`|`/styles`|g' "$tmp/site/src/reference/web.md"
  expect_fail ac5-web ac5_routes_documented "$tmp/site/src" crates

  fresh; sed -i '/^\*Verified against:/d' "$tmp/site/src/flows/auth.md"
  expect_fail ac6 ac6_pages_stamped "$tmp/site/src"

  # AC7: zero items evaluated is a failure.
  expect_fail ac7-links ac2_links_resolve "$tmp/empty"
  expect_fail ac7-env ac4_env_documented "$SITE/src" "$tmp/empty"
  expect_fail ac7-routes ac5_routes_documented "$SITE/src" "$tmp/empty"
  expect_fail ac7-pages ac6_pages_stamped "$tmp/empty"

  ((st_bad == 0)) || return 1
  echo "docs-check selftest: every check fails on its mutation"
}

case "${1:-}" in
  --selftest) selftest ;;
  "")
    usage_dir=$(mktemp -d)
    trap 'rm -rf "$usage_dir"' EXIT
    collect_usage "$usage_dir"
    run_all "$SITE" crates "$usage_dir"
    echo "docs-check: site in step with the code"
    ;;
  *) echo "usage: docs-check.sh [--selftest]" >&2; exit 2 ;;
esac
