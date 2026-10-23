# files (read, write)
- Paths are relative to your session folder. Nothing outside it is reachable.
- `read` returns up to 200 lines from `offset` (1-based). For long files, find the spot first: `sh` with `grep -n pattern file`, then `read` with that offset.
- `write` creates or replaces the whole file. For a small change in a big file, use `sh` with `python3 - <<'EOF' ... EOF` or `sed -i`, then check with `grep -n` or `read`.
- Make folders with `write` (it creates parents) or `mkdir -p`.
- Show the user short results, not whole files. Quote only the lines that matter.
