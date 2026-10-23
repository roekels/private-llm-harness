# python
- python3 is installed. Debian blocks system-wide `pip install`. Use a venv in your folder:
  `python3 -m venv .venv && . .venv/bin/activate && pip install -q PACKAGE && python script.py` (in one sh call).
- Needs network for pip. If it's off, use only the standard library.
- For quick scripts, use a heredoc: `python3 - <<'EOF' ... EOF`. For anything reused, `write` a .py file and run it.
- Print short results. Save large data to files and print a summary (row counts, first rows).
