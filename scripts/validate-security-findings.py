#!/usr/bin/env python3
"""Consistency validator for docs/security/findings.json.

Run from the repository root:

    python3 scripts/validate-security-findings.py

Exits 0 when the register is internally consistent, 1 otherwise, printing one
line per violation. This is a documentation-integrity check: it does not verify
that the security claims are true, only that the register is well-formed,
self-consistent, and carries the evidence each status is required to supply.
"""
from __future__ import annotations

import json
import pathlib
import re
import sys

REGISTER = pathlib.Path("docs/security/findings.json")

ALLOWED_STATUSES = {
    "CONFIRMED_CURRENT",
    "PARTIALLY_FIXED",
    "FIXED_AND_VERIFIED",
    "NOT_REACHABLE",
    "FALSE_POSITIVE",
    "REQUIRES_RUNTIME_VERIFICATION",
    "DEFERRED_PRODUCT_DECISION",
}

ALLOWED_SEVERITIES = {"critical", "high", "medium", "low"}

# Evidence each status must carry, beyond the fields common to all findings.
REQUIRED_BY_STATUS = {
    "CONFIRMED_CURRENT": ["reachable_from", "reproducer"],
    "PARTIALLY_FIXED": ["remaining_exposure"],
    "FIXED_AND_VERIFIED": ["fixing_prs", "verification_tests"],
    "NOT_REACHABLE": ["unreachable_because"],
    "FALSE_POSITIVE": [],
    "REQUIRES_RUNTIME_VERIFICATION": ["required_dynamic_test"],
    "DEFERRED_PRODUCT_DECISION": ["decision_required"],
}

# Statuses that must name concrete follow-up work.
FOLLOW_UP_REQUIRED = {
    "CONFIRMED_CURRENT",
    "PARTIALLY_FIXED",
    "DEFERRED_PRODUCT_DECISION",
    "REQUIRES_RUNTIME_VERIFICATION",
}

SHA_RE = re.compile(r"^[0-9a-f]{40}$")
ABSOLUTE_DEV_PATH_RE = re.compile(r"/home/[^\s\"]+|/root/SoulSystem|/root/soul_system")


def nonempty(value) -> bool:
    if value is None:
        return False
    if isinstance(value, (str, list, dict)):
        return len(value) > 0
    return True


def main() -> int:
    if not REGISTER.is_file():
        print(f"FAIL: {REGISTER} not found (run from the repository root)")
        return 1

    raw = REGISTER.read_text(encoding="utf-8")
    try:
        doc = json.loads(raw)
    except json.JSONDecodeError as exc:
        print(f"FAIL: {REGISTER} is not valid JSON: {exc}")
        return 1

    errors: list[str] = []
    findings = doc.get("findings")
    if not isinstance(findings, list) or not findings:
        print("FAIL: 'findings' must be a non-empty list")
        return 1

    # ── Top-level metadata ────────────────────────────────────────────────
    for key in ("audit_source_baseline", "reverified_against_main", "reverified_at_utc",
                "toolchain", "method", "count", "status_summary", "severity_summary"):
        if not nonempty(doc.get(key)):
            errors.append(f"metadata: missing or empty '{key}'")

    for key in ("audit_source_baseline", "previous_reverification_baseline", "reverified_against_main"):
        val = doc.get(key)
        if val is not None and not SHA_RE.match(str(val)):
            errors.append(f"metadata: '{key}' is not a full 40-char SHA: {val!r}")

    # ── Arithmetic ────────────────────────────────────────────────────────
    count = doc.get("count")
    if count != len(findings):
        errors.append(f"count mismatch: count={count} but {len(findings)} findings present")

    status_summary = doc.get("status_summary", {})
    actual_status: dict[str, int] = {}
    for f in findings:
        actual_status[f.get("status")] = actual_status.get(f.get("status"), 0) + 1
    for status, n in actual_status.items():
        if status_summary.get(status, 0) != n:
            errors.append(f"status_summary['{status}']={status_summary.get(status, 0)} but actual={n}")
    if sum(status_summary.values()) != len(findings):
        errors.append(f"status_summary totals {sum(status_summary.values())} != {len(findings)} findings")

    severity_summary = doc.get("severity_summary", {})
    actual_sev: dict[str, int] = {}
    for f in findings:
        actual_sev[f.get("severity")] = actual_sev.get(f.get("severity"), 0) + 1
    for sev, n in actual_sev.items():
        if severity_summary.get(sev, 0) != n:
            errors.append(f"severity_summary['{sev}']={severity_summary.get(sev, 0)} but actual={n}")
    if sum(severity_summary.values()) != len(findings):
        errors.append(f"severity_summary totals {sum(severity_summary.values())} != {len(findings)} findings")

    # severity_status_summary, when present, must agree per severity.
    sss = doc.get("severity_status_summary")
    if isinstance(sss, dict):
        for sev, per_status in sss.items():
            total = sum(per_status.values())
            if total != actual_sev.get(sev, 0):
                errors.append(
                    f"severity_status_summary['{sev}'] totals {total} but {actual_sev.get(sev, 0)} findings have that severity"
                )

    # ── Per-finding ───────────────────────────────────────────────────────
    seen: set[str] = set()
    for f in findings:
        fid = f.get("id", "<no-id>")
        if fid in seen:
            errors.append(f"{fid}: duplicate id")
        seen.add(fid)

        status = f.get("status")
        if status not in ALLOWED_STATUSES:
            errors.append(f"{fid}: status {status!r} not in the allowed enum")
            continue

        if f.get("severity") not in ALLOWED_SEVERITIES:
            errors.append(f"{fid}: severity {f.get('severity')!r} not in {sorted(ALLOWED_SEVERITIES)}")

        for key in ("title", "current_state", "required_invariant", "residual_risk", "reverified_against"):
            if not nonempty(f.get(key)):
                errors.append(f"{fid}: missing or empty '{key}'")

        if f.get("reverified_against") != doc.get("reverified_against_main"):
            errors.append(f"{fid}: reverified_against does not match the register's reverified_against_main")

        for key in REQUIRED_BY_STATUS[status]:
            if not nonempty(f.get(key)):
                errors.append(f"{fid}: status {status} requires a non-empty '{key}'")

        if status in FOLLOW_UP_REQUIRED and not nonempty(f.get("follow_up")):
            errors.append(f"{fid}: status {status} requires a concrete 'follow_up'")

        # Fixed findings must cite at least one runnable test with a command.
        if status == "FIXED_AND_VERIFIED":
            for test in f.get("verification_tests") or []:
                if not nonempty(test.get("name")) or not nonempty(test.get("command")):
                    errors.append(f"{fid}: verification_tests entry missing 'name' or 'command'")

        # Every fixing PR must carry a merge SHA and a number.
        for pr in f.get("fixing_prs") or []:
            if not isinstance(pr.get("number"), int) or pr["number"] <= 0:
                errors.append(f"{fid}: fixing_prs entry has no valid PR number")
            if not SHA_RE.match(str(pr.get("merge_commit", ""))):
                errors.append(f"{fid}: fixing_prs #{pr.get('number')} has no full 40-char merge_commit")

        # Paths must be repository-relative.
        for path in f.get("current_files") or []:
            if path.startswith("/"):
                errors.append(f"{fid}: current_files entry is absolute, must be repo-relative: {path}")

    # ── Stale developer paths anywhere in the file ────────────────────────
    for lineno, line in enumerate(raw.splitlines(), start=1):
        hit = ABSOLUTE_DEV_PATH_RE.search(line)
        if hit:
            errors.append(f"line {lineno}: stale absolute developer path {hit.group(0)!r}")

    if errors:
        print(f"FAIL: {len(errors)} consistency violation(s) in {REGISTER}")
        for err in errors:
            print(f"  - {err}")
        return 1

    print(f"OK: {REGISTER} is consistent ({len(findings)} findings)")
    for status in sorted(actual_status):
        print(f"  {status:30} {actual_status[status]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
