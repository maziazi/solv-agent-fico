#!/usr/bin/env python3
"""Recompute a Solv receipt's evidence_sha256 using only the Python stdlib.

This is the reference verifier for docs/SPEC.md section 6.3: it shares no code
with the Rust service, so a match means a third party can reproduce the hash.

Usage:
  recompute_receipt.py [FILE|-]               verify a receipt JSON (default: stdin)
  recompute_receipt.py --canonical [FILE|-]   print the canonical text and its
                                              SHA-256 of an arbitrary JSON document

Exit status: 0 hash matches, 1 mismatch, 2 unreadable input.
"""
import hashlib
import json
import sys


def _reject_constant(name):
    raise ValueError("non-finite number is not valid JSON: " + name)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def sha256_hex(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def load(path):
    raw = sys.stdin.buffer.read() if path in (None, "-") else open(path, "rb").read()
    return json.loads(raw.decode("utf-8"), parse_constant=_reject_constant)


def main(argv):
    args = argv[1:]
    print_canonical = False
    if args and args[0] == "--canonical":
        print_canonical = True
        args = args[1:]
    if len(args) > 1:
        sys.stderr.write(__doc__)
        return 2
    try:
        document = load(args[0] if args else None)
    except (OSError, ValueError) as error:
        sys.stderr.write("cannot read input: %s\n" % error)
        return 2

    if print_canonical:
        text = canonical(document)
        print(text)
        print(sha256_hex(text))
        return 0

    try:
        claimed = document["evidence_sha256"]
        evidence = document["evidence"]
    except (KeyError, TypeError):
        sys.stderr.write("input has no evidence_sha256 / evidence fields\n")
        return 2
    recomputed = sha256_hex(canonical(evidence))
    print("claimed   :", claimed)
    print("recomputed:", recomputed)
    if recomputed == claimed:
        print("MATCH")
        return 0
    print("MISMATCH")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
