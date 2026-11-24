#!/bin/sh
# The contributor docs name real files: every repo path that AGENTS.md, CONTRIBUTING.md,
# SECURITY.md, README.md, .github's templates and docs/NOW.md name is in the repo, and every
# backlog label in docs/NOW.md is used once. A doc that names a file that moved fails here, not in
# a stranger's hands.
#
# What's read as a path: a backticked token (its first word only: a command's arguments aren't
# paths; a leading `./` is dropped) and a relative link's target. Skipped: routes (`/status`), flags,
# tokens holding `=`, `[`, `<`, `*`, `…` or `?`, URLs, and paths another repo owns: written with its
# owner first (`AnthonE/Gates/...`), straight after its name (Gates' `ci/quick.sh`), or beside this
# checkout (`../Gates`). A file's path under a top-level folder we don't have is a warning, not a
# failure: a typo, or another repo's path written without its name. A bare `NAME.md` is looked for at
# the root, in `docs/` and in `.github/`. "In the repo" means tracked, or new and not ignored; a path
# git ignores is a build's output (`web/dist`) and is taken as named.
#
#   sh scripts/refs.sh            # the docs above
#   sh scripts/refs.sh FILE...    # just these
set -eu
cd "$(dirname "$0")/.."

if [ "$#" -gt 0 ]; then
  files="$*"
else
  files=""
  for f in AGENTS.md CONTRIBUTING.md SECURITY.md README.md .github/*.md .github/ISSUE_TEMPLATE/*.md docs/NOW.md; do
    [ -f "$f" ] && files="$files $f"
  done
fi

if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  in_repo() { git ls-files --cached --others --exclude-standard -- "$1" 2>/dev/null | grep -q . || git check-ignore -q -- "$1/"; }
else
  in_repo() { [ -e "$1" ]; }
fi

bad=0
out=$(mktemp)
trap 'rm -f "$out"' EXIT

# One record per reference: file, line, kind (code|link), token, and 1 if the text just before it
# (on its line, or at the end of the line before) names another repo.
# shellcheck disable=SC2086
awk -v q="'" '
  BEGIN { theirs = "(Gates|oneshot|scry-forge|Fireplace|elo-pros-launcher|elopros[.]com)(" q "s|" q ")? *:? *$" }
  function other(before) { return before ~ theirs }
  FNR == 1 { fence = 0; prev = "" }
  /^[ \t]*```/ { fence = !fence; prev = ""; next }
  fence { next }
  {
    s = $0
    while (match(s, /\]\([^)]*\)/)) {
      printf "%s\t%d\tlink\t%s\t0\n", FILENAME, FNR, substr(s, RSTART + 2, RLENGTH - 3)
      s = substr(s, RSTART + RLENGTH)
    }
    # A repo name ending the line before still owns the first token on this line.
    s = $0; before = prev " "
    while ((i = index(s, "`")) > 0) {
      rest = substr(s, i + 1)
      j = index(rest, "`")
      if (j == 0) break
      tok = substr(rest, 1, j - 1)
      before = before substr(s, 1, i - 1)
      printf "%s\t%d\tcode\t%s\t%d\n", FILENAME, FNR, tok, other(before)
      before = before "`" tok "`"
      s = substr(rest, j + 1)
    }
    prev = $0
  }
' $files >"$out"

tab=$(printf '\t')
while IFS="$tab" read -r file line kind tok foreign; do
  [ "$foreign" = 1 ] && continue
  case "$tok" in
    "" | /* | -* | http* | mailto:* | \#* | AnthonE/* | ~* | .) continue ;;
    *=* | *\[* | *\<* | *\** | *…* | *\?* | *\{* | *\$*) continue ;;
  esac
  w=${tok%% *}
  w=${w#./}
  w=${w%%#*}
  w=$(printf '%s' "$w" | sed 's/:[0-9][0-9,-]*$//')
  [ -n "$w" ] || continue
  if [ "$kind" = link ]; then
    dir=$(dirname "$file")
    [ "$dir" = . ] && p=$w || p="$dir/$w"
    # docs/../README.md → README.md
    p=$(printf '%s' "$p" | sed -e 's#^\./##' -e ':a' -e 's#[^/][^/]*/\.\./##' -e 'ta')
    p=${p%/}
    in_repo "$p" || { echo "$file:$line: the link to $w names no file in the repo"; bad=1; }
    continue
  fi
  case "$w" in
    .. | ../*) ;; # beside this checkout: another repo's (`../Gates`)
    */*)
      seg=${w%%/*}
      if [ -e "$seg" ] && [ "$seg" != . ] && [ "$seg" != .. ]; then
        p=$w
      elif [ "$seg" != . ] && [ "$seg" != .. ] && [ -d "crates/$seg" ]; then
        p="crates/$w"
      else
        case "${w##*/}" in
          ?*.?*) echo "$file:$line: warning: \`$w\` is under no top-level folder of this repo (a typo, or another repo's path without its name?)" >&2 ;;
        esac
        continue
      fi
      p=${p%/}
      in_repo "$p" || { echo "$file:$line: \`$w\` names no file in the repo"; bad=1; }
      ;;
    *.md)
      in_repo "$w" || in_repo "docs/$w" || in_repo ".github/$w" ||
        { echo "$file:$line: \`$w\` is in neither the root, docs/ nor .github/"; bad=1; }
      ;;
  esac
done <"$out"

# The backlog's labels are stable and used once: code and pull requests cite `NOW.md §<label>`.
if [ -f docs/NOW.md ]; then
  dups=$(sed -n 's/^## \([^ ]*\) · .*/\1/p' docs/NOW.md | sort | uniq -d)
  for d in $dups; do
    echo "docs/NOW.md: the label $d is used more than once"
    bad=1
  done
fi

[ "$bad" = 0 ] && echo "refs: OK"
exit "$bad"
