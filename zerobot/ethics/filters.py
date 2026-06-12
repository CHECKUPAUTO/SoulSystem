"""Ethical safety filters for ZeroBot.

Implements:
1. Pre-generation filter: detect malicious intent before invoking the LLM
2. Binary authorization: verify SHA256 against allowlist
3. Post-generation filter: strip finalized exploit code, verify educational-only content
"""

from __future__ import annotations

import hashlib
import logging
import re
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Optional

logger = logging.getLogger(__name__)


class IntentClassification(Enum):
    """Classification of user intent."""

    AUTHORIZED_RESEARCH = "authorized_research"
    EDUCATIONAL = "educational"
    CTF = "ctf"
    MALICIOUS_EXPLICIT = "malicious_explicit"
    MALICIOUS_DISGUISED = "malicious_disguised"
    UNCLEAR = "unclear"


@dataclass
class FilterResult:
    """Result from a safety filter check."""

    blocked: bool
    reason: str = ""
    classification: IntentClassification = IntentClassification.UNCLEAR
    details: dict = field(default_factory=dict)


# Patterns that indicate malicious intent (unauthorized targeting)
MALICIOUS_PATTERNS: list[tuple[re.Pattern, str]] = [
    (re.compile(r"(?i)\b(crack|warez|pirate|illegal)\b"), "references to illegal activity"),
    (re.compile(r"(?i)\b(hack|attack|breach|intrude|crack|break)\b(?:\s+\w+){0,4}\s+(government|bank|military|hospital|school)(?!\s*(ctf|lab|challenge|educational|paper|thesis|study|research))"), "unauthorized targeting of protected entities"),
    (re.compile(r"(?i)\b(steal|exfiltrate|dump|leak)\s+(data|passwords?|credentials?|databases?|secrets?)\b"), "data theft intent"),
    (re.compile(r"(?i)\b(without|no|bypass|avoid)\s+(permission|authorization|consent|auth)\b"), "attempted authorization bypass"),
    (re.compile(r"(?i)\bransom\b.*\b(encrypt|lock|block)\b"), "ransomware intent"),
    (re.compile(r"(?i)\b(social.engineer|phish|spear.phish|vish)\b.*\b(employee|user|customer)\b"), "social engineering targeting"),
    (re.compile(r"(?i)\b(anonymous|protest|hacktivist)\b"), "hacktivist references"),
]

# Patterns that indicate educational/authorized context
EDUCATIONAL_PATTERNS: list[re.Pattern] = [
    re.compile(r"(?i)\b(ctf|challenge|lab|educational|learning|training|course)\b"),
    re.compile(r"(?i)\b(bug.bounty|vdp|vulnerability.disclosure)\b"),
    re.compile(r"(?i)\b(authorized|permission|consent|allowlist|pentest|penetration.test)\b"),
    re.compile(r"(?i)\b(academic|research|paper|thesis|study)\b"),
    re.compile(r"(?i)\b(sandbox|vm|virtual.machine|local|isolated)\b"),
]

# Patterns to strip from final output (post-generation)
EXPLOIT_CODE_PATTERNS: list[re.Pattern] = [
    re.compile(r"exec\(\s*['\"]/bin/sh['\"]\s*\)"),
    re.compile(r"system\(\s*['\"]/bin/sh['\"]\s*\)"),
    re.compile(r"os\.system\(\s*['\"].*?(rm|wget|curl|chmod|nc|bash).*?['\"]\s*\)"),
]


class EthicalFilter:
    """Pre- and post-generation ethical safety filter."""

    def __init__(self, allowed_hashes: Optional[list[str]] = None):
        self.allowed_hashes = set(allowed_hashes or [])
        self._wildcard_allowed = "*" in self.allowed_hashes

    # ── Pre-generation: Intent Classification ───────────────────────

    def classify_intent(self, message: str) -> tuple[IntentClassification, list[str]]:
        """Classify user intent from their message."""
        flags: list[str] = []

        # Check for educational/authorized context (takes precedence for ambiguous cases)
        edu_matches = [p.search(message) for p in EDUCATIONAL_PATTERNS]
        has_educational_context = any(edu_matches)
        if has_educational_context:
            flags.extend(m.group() for m in edu_matches if m)

        # Check for malicious patterns
        malicious_matches = []
        for pattern, desc in MALICIOUS_PATTERNS:
            m = pattern.search(message)
            if m:
                malicious_matches.append(desc)
                flags.append(desc)

        # Check if the user explicitly claims authorization
        has_authorization_claim = bool(
            re.search(r"(?i)\b(i\s+have|i\s+got|with)\s+(authorization|permission|consent|approval)\b", message)
        )

        # Classify
        if malicious_matches and not has_educational_context and not has_authorization_claim:
            return IntentClassification.MALICIOUS_EXPLICIT, flags

        if malicious_matches and (has_educational_context or has_authorization_claim):
            return IntentClassification.MALICIOUS_DISGUISED, flags

        if has_educational_context:
            return IntentClassification.EDUCATIONAL, flags

        return IntentClassification.UNCLEAR, flags

    def pre_generation_filter(self, user_message: str) -> FilterResult:
        """Check user message before LLM invocation."""
        classification, flags = self.classify_intent(user_message)

        if classification in (IntentClassification.MALICIOUS_EXPLICIT,):
            return FilterResult(
                blocked=True,
                reason="I cannot assist with unauthorized security testing or attacks. "
                "As an ethical vulnerability research assistant, I require documented "
                "authorization before analyzing any target. If you have a legitimate "
                "educational or authorized testing need, please clarify the context "
                "(e.g., CTF, authorized pentest, academic research).",
                classification=classification,
                details={"flags": flags},
            )

        return FilterResult(
            blocked=False,
            classification=classification,
            details={"flags": flags},
        )

    # ── Binary Authorization ────────────────────────────────────────

    def authorize_binary(self, binary_path: str | Path) -> FilterResult:
        """Check if a binary is authorized for analysis."""
        if self._wildcard_allowed:
            return FilterResult(
                blocked=False,
                classification=IntentClassification.AUTHORIZED_RESEARCH,
                details={"note": "wildcard allowlist — all binaries accepted"},
            )

        try:
            sha256 = hashlib.sha256()
            with open(binary_path, "rb") as f:
                for chunk in iter(lambda: f.read(65536), b""):
                    sha256.update(chunk)
            file_hash = sha256.hexdigest()
        except FileNotFoundError:
            return FilterResult(
                blocked=True,
                reason="Binary file not found.",
                details={"error": "file_not_found"},
            )

        if file_hash in self.allowed_hashes:
            return FilterResult(
                blocked=False,
                classification=IntentClassification.AUTHORIZED_RESEARCH,
                details={"hash": file_hash},
            )

        return FilterResult(
            blocked=True,
            reason=f"Binary with hash {file_hash[:16]}... is not in the authorized allowlist. "
            "Only explicitly authorized binaries can be deeply analyzed. "
            "You can add this hash to the session allowlist if you have authorization.",
            details={"hash": file_hash},
        )

    # ── Post-generation: Output Safety ──────────────────────────────

    def post_generation_filter(self, text: str) -> FilterResult:
        """Sanitize LLM output to remove finalized exploit code."""
        issues: list[str] = []

        for pattern in EXPLOIT_CODE_PATTERNS:
            if pattern.search(text):
                issues.append(f"Matched exploit pattern: {pattern.pattern}")

        if issues:
            return FilterResult(
                blocked=True,
                reason="Output contained executable exploit code patterns.",
                details={"issues": issues},
            )

        return FilterResult(
            blocked=False,
            classification=IntentClassification.EDUCATIONAL,
        )

    # ── Allowlist Management ────────────────────────────────────────

    def add_to_allowlist(self, file_hash: str) -> None:
        """Add a hash to the allowlist."""
        self.allowed_hashes.add(file_hash)
        logger.info("Added hash to allowlist: %s", file_hash[:16])

    def remove_from_allowlist(self, file_hash: str) -> None:
        """Remove a hash from the allowlist."""
        self.allowed_hashes.discard(file_hash)
