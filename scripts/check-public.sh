#!/bin/bash
# Blocks commits that would publish private details or secrets.
# - private/denylist.txt (gitignored, optional): your own private strings, one per line.
# - Built-in: secret-shaped strings (API keys, tokens, private keys).
# Usage: scripts/check-public.sh            checks staged files (git pre-commit hook)
#        scripts/check-public.sh --all      checks every tracked and untracked, non-ignored file
set -u
cd "$(git rev-parse --show-toplevel)" || exit 1
if [ "${1:-}" = "--all" ]; then
  files=$(git ls-files --cached --others --exclude-standard)
else
  files=$(git diff --cached --name-only --diff-filter=ACM)
fi
[ -z "$files" ] && exit 0

fail=0
deny=private/denylist.txt
if [ -f "$deny" ]; then
  patterns=$(grep -v '^\s*#' "$deny" | sed '/^\s*$/d')
  if [ -n "$patterns" ]; then
    hits=$(echo "$files" | tr '\n' '\0' | xargs -0 grep -I -n -i -F -f <(echo "$patterns") 2>/dev/null)
    if [ -n "$hits" ]; then
      echo "check-public: private strings found (from $deny):" >&2
      echo "$hits" | cut -c1-160 >&2
      fail=1
    fi
  fi
fi
secret='(sk-[A-Za-z0-9_-]{20,}|ghp_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,}|cf(ut|at)_[A-Za-z0-9]{20,}|xox[baprs]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----)'
hits=$(echo "$files" | tr '\n' '\0' | xargs -0 grep -I -n -E "$secret" 2>/dev/null)
if [ -n "$hits" ]; then
  echo "check-public: something that looks like a secret:" >&2
  echo "$hits" | cut -c1-160 >&2
  fail=1
fi
[ "$fail" = 0 ] || echo "check-public: commit blocked. Remove these, or add the file to .gitignore." >&2
exit $fail
