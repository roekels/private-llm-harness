# terminal (sh)
- bash in your session folder on Debian Linux, as an unprivileged user. No sudo, no TTY: never run interactive programs (vim, less, top, `read`). Use flags like `-y`, `--yes`, `--no-pager`.
- Each call is fresh: `cd` and variables don't carry over. Chain steps with `&&` in one call.
- 60 s limit per call, and anything left running (servers started with `&`) is killed afterwards. Split long jobs into steps.
- Output over ~2 KB is clipped to head and tail; the full text is saved to `.harness/out/<id>.txt`. Ask for less: `| head -50`, `| tail -20`, `wc -l`, `grep -n`, `rg`.
- Network may be off. If `curl` fails with "Could not resolve host", say so instead of retrying.
- Tools: python3, node/npm, git, curl, wget, jq, rg, unzip. For Python packages, see skill python.
- Don't repeat a failing command unchanged. Read the error, change one thing.
