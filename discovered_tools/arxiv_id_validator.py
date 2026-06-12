#!/usr/bin/env python3
"""Validate and normalize arXiv IDs according to arXiv v2 specification."""

import argparse
import re
import sys


def validate_arxiv_id(arxiv_id: str) -> tuple[bool, str | None]:
    """Validate an arXiv ID and return (is_valid, normalized_id or error_msg)."""
    # Remove whitespace and normalize case
    arxiv_id = arxiv_id.strip()
    
    # Pattern for new arXiv IDs (2007+): category/year/month/identifier
    # e.g., 2401.12345, 2401.12345v1, 2401.12345v12
    new_pattern = r'^(\d{4})\.(\d{5})(?:v(\d+))?$'
    
    # Pattern for old arXiv IDs (pre-2007): category/YYMMDDxxx
    # e.g., math/0601001, cs/0112001v1
    old_pattern = r'^([a-z]{2}(?:-[a-z]{2})?)/\d{6}(?:v(\d+))?$'
    
    # Try new format
    match = re.match(new_pattern, arxiv_id)
    if match:
        year, num, version = match.groups()
        # Basic sanity checks
        year_int = int(year)
        if not (1900 <= year_int <= 2100):
            return False, f"Invalid year: {year}"
        return True, f"{year}.{num}{'v' + version if version else ''}"
    
    # Try old format
    match = re.match(old_pattern, arxiv_id)
    if match:
        category, version = match.groups()
        # Validate category (common ones, not exhaustive)
        valid_categories = {'cs', 'math', 'physics', 'q-bio', 'q-fin', 'stat', 'cond-mat', 'gr-qc', 'hep-ex', 'hep-lat', 'hep-ph', 'hep-th', 'nucl-ex', 'nucl-th', 'astro-ph'}
        main_cat = category.split('-')[0]
        if main_cat not in valid_categories:
            return False, f"Invalid category: {category}"
        return True, f"{category}{'v' + version if version else ''}"
    
    return False, "Does not match arXiv ID format (new or old)"


def main():
    parser = argparse.ArgumentParser(
        description="Validate and normalize arXiv IDs per arXiv v2 specification."
    )
    parser.add_argument('ids', nargs='+', help="arXiv IDs to validate")
    parser.add_argument('--quiet', '-q', action='store_true', help="Only output valid IDs")
    args = parser.parse_args()
    
    for arxiv_id in args.ids:
        is_valid, result = validate_arxiv_id(arxiv_id)
        if is_valid:
            print(f"✓ {arxiv_id} → {result}")
        else:
            if not args.quiet:
                print(f"✗ {arxiv_id} → {result}", file=sys.stderr)
            else:
                print(f"{arxiv_id}", file=sys.stderr)  # still report invalid in stderr


if __name__ == "__main__":
    main()
