#!/usr/bin/env python3
"""Atom Feed Validator - Validates Atom 1.0 feeds against W3C specifications."""
import argparse
import sys
import xml.etree.ElementTree as ET
from urllib.request import urlopen
from urllib.error import URLError, HTTPError
from datetime import datetime

NS = {'atom': 'http://www.w3.org/2005/Atom'}

def validate_feed(feed_url: str) -> tuple[bool, list[str]]:
    """Validate an Atom feed from URL. Returns (is_valid, list_of_errors)."""
    errors = []
    try:
        with urlopen(feed_url) as response:
            xml_data = response.read()
    except (URLError, HTTPError) as e:
        return False, [f"Failed to fetch feed: {e}"]

    try:
        root = ET.fromstring(xml_data)
    except ET.ParseError as e:
        return False, [f"XML parsing error: {e}"]

    # Must be an atom:feed element
    if root.tag != '{http://www.w3.org/2005/Atom}feed':
        errors.append(f"Root element must be 'feed', found '{root.tag.split('}')[1] if '}' in root.tag else root.tag}'")
        return False, errors

    # Required elements: title, id, updated
    required = ['title', 'id', 'updated']
    for req in required:
        elem = root.find(f'atom:{req}', NS)
        if elem is None or not elem.text or not elem.text.strip():
            errors.append(f"Missing or empty required element: <atom:{req}>")

    # Validate date format for 'updated' (ISO 8601)
    updated_elem = root.find('atom:updated', NS)
    if updated_elem is not None and updated_elem.text:
        try:
            datetime.fromisoformat(updated_elem.text.replace('Z', '+00:00'))
        except ValueError:
            errors.append(f"Invalid date format in <atom:updated>: '{updated_elem.text}' (expected ISO 8601)")

    # Validate 'id' is a URI
    id_elem = root.find('atom:id', NS)
    if id_elem is not None and id_elem.text:
        if not (id_elem.text.startswith('http://') or id_elem.text.startswith('https://') or id_elem.text.startswith('urn:')):
            errors.append(f"<atom:id> should be a URI, got: '{id_elem.text}'")

    # Check at least one entry if feed is not empty (optional per spec, but common practice)
    entries = root.findall('atom:entry', NS)
    if len(entries) == 0:
        errors.append("Feed contains no entries (optional but recommended)")

    return len(errors) == 0, errors

def main():
    parser = argparse.ArgumentParser(description="Validate Atom 1.0 feeds against W3C Atom specification.")
    parser.add_argument('url', help="URL of the Atom feed to validate")
    args = parser.parse_args()

    is_valid, errors = validate_feed(args.url)
    if is_valid:
        print("✅ Feed is valid.")
        sys.exit(0)
    else:
        print("❌ Feed is invalid:")
        for err in errors:
            print(f"  - {err}")
        sys.exit(1)

if __name__ == "__main__":
    main()