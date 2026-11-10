#!/usr/bin/env python3
"""Optional AI review of installer / doctor output, for things a script cannot judge by itself
("does this log look like a healthy install?", "is this warning a real problem on Fedora?").

It is a second opinion, never a gate by default: the deterministic tests (scripts/tests/run.sh)
decide pass or fail. This only prints what the model noticed.

  DEEPSEEK_API_KEY=... scripts/ci/ai_review.py install.log [more.log ...] [--distro "Ubuntu 24.04"] [--strict]

* The key comes from the environment only (a CI secret). Never put it in the repository or in a log.
* Without a key the script says so and exits 0, so CI works for forks and for people without one.
* Everything is redacted before it leaves the machine: home paths, user names, IP addresses,
  e-mail addresses, device serials and anything that looks like a token.
* `--strict` makes the exit code 1 when the model's verdict line is `VERDICT: FAIL`.
"""
import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.request

API = "https://api.deepseek.com/chat/completions"
MODEL = os.environ.get("DEEPSEEK_MODEL", "deepseek-flash")
MAX_CHARS = 24_000

REDACTIONS = [
    (re.compile(r"/home/[^/\s]+"), "/home/USER"),
    (re.compile(r"/Users/[^/\s]+"), "/Users/USER"),
    (re.compile(r"\b\d{1,3}(?:\.\d{1,3}){3}(?::\d+)?\b"), "IP"),
    (re.compile(r"[\w.+-]+@[\w-]+\.[\w.-]+"), "EMAIL"),
    (re.compile(r"\b(?:sk|ghp|gho|ghs|xox[bp])-?[A-Za-z0-9_]{16,}\b"), "TOKEN"),
    (re.compile(r"\b[A-Z0-9]{10,16}\b"), "SERIAL"),  # adb serials look like R52X708AEHK
]


def redact(text: str) -> str:
    """Removes personal data from [text] before it leaves the machine: home paths, IP addresses, e-mail addresses, tokens and adb serials."""
    for pattern, repl in REDACTIONS:
        text = pattern.sub(repl, text)
    return text


def ask(key: str, distro: str, logs: str) -> str:
    """Asks the AI service to review [logs] (already redacted) of one distribution and returns its answer; the last line is expected to be `VERDICT: OK` or `VERDICT: FAIL`."""
    system = (
        "You review the output of a Linux installer and its tests for UScreen, a tool that turns an "
        "Android tablet into a second display or a graphics tablet. Report only real problems: "
        "errors that were hidden by `|| true`, steps that silently did nothing, wrong package "
        "names for the distribution, or a restart that is claimed but not needed (or needed but not "
        "claimed). Be brief: a bullet list, then a last line exactly `VERDICT: OK` or `VERDICT: FAIL`."
    )
    body = {
        "model": MODEL,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": f"Distribution: {distro}\n\n{logs}"},
        ],
        "temperature": 0,
        "stream": False,
    }
    req = urllib.request.Request(
        API,
        data=json.dumps(body).encode(),
        headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=60) as resp:
        data = json.load(resp)
    return data["choices"][0]["message"]["content"]


def main() -> int:
    """Reads the log files, redacts them with [redact] and asks for a review. Returns a process exit code; with --strict a FAIL verdict is a failure."""
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("logs", nargs="+")
    ap.add_argument("--distro", default="unknown")
    ap.add_argument("--strict", action="store_true")
    ap.add_argument("--print-redacted", action="store_true", help="show what would be sent, send nothing")
    args = ap.parse_args()

    parts = []
    for path in args.logs:
        with open(path, errors="replace") as f:
            parts.append(f"--- {os.path.basename(path)} ---\n{f.read()}")
    logs = redact("\n".join(parts))[-MAX_CHARS:]

    if args.print_redacted:
        print(logs)
        return 0
    key = os.environ.get("DEEPSEEK_API_KEY", "").strip()
    if not key:
        print("AI review skipped: DEEPSEEK_API_KEY is not set")
        return 0
    try:
        answer = ask(key, args.distro, logs)
    except (urllib.error.URLError, KeyError, ValueError) as e:
        print(f"AI review unavailable ({type(e).__name__}); the deterministic tests are unaffected")
        return 0
    print(answer)
    return 1 if args.strict and "VERDICT: FAIL" in answer else 0


if __name__ == "__main__":
    sys.exit(main())
