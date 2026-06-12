#!/usr/bin/env python3
"""
Cambrian Fossil Analyzer

This tool implements a simplified version of quantitative paleontological analysis
used to assess Cambrian fossil diversity and disparity, inspired by recent
discoveries (e.g., QuamMagazine 2026). It estimates taxonomic richness and
morphological disparity from fossil occurrence data.

Usage:
  python cambrian_fossil_analyzer.py --input fossils.csv --output report.json
"""

import argparse
import csv
import json
from pathlib import Path
from typing import Dict, List, Tuple
import sys


def load_fossils(filepath: str) -> List[Dict]:
    """Load fossil records from CSV file."""
    with open(filepath, newline='', encoding='utf-8') as f:
        reader = csv.DictReader(f)
        return list(reader)


def estimate_richness(fossils: List[Dict]) -> int:
    """Estimate taxonomic richness as number of unique genera."""
    return len(set(f['genus'] for f in fossils if f.get('genus')))


def compute_disparity(fossils: List[Dict]) -> float:
    """
    Compute morphological disparity as mean pairwise Euclidean distance
    in morphospace defined by body length and segmentation count.
    """
    points = []
    for f in fossils:
        try:
            length = float(f['body_length_cm'])
            segments = float(f['segment_count'])
            points.append((length, segments))
        except (ValueError, KeyError):
            continue
    if len(points) < 2:
        return 0.0
    
    total = 0.0
    count = 0
    for i in range(len(points)):
        for j in range(i + 1, len(points)):
            diff0 = points[i][0] - points[j][0]
            diff1 = points[i][1] - points[j][1]
            total += (diff0 ** 2 + diff1 ** 2) ** 0.5
            count += 1
    return total / count if count > 0 else 0.0


def analyze_site(fossils: List[Dict]) -> Dict:
    """Perform full analysis on fossil dataset."""
    richness = estimate_richness(fossils)
    disparity = compute_disparity(fossils)
    
    # Stratigraphic range (min/max age in Ma)
    ages = []
    for f in fossils:
        try:
            ages.append(float(f['age_ma']))
        except (ValueError, KeyError):
            continue
    age_range = (min(ages), max(ages)) if ages else (None, None)
    
    return {
        "site_name": "Cambrian Fossil Assemblage",
        "total_specimens": len(fossils),
        "taxonomic_richness": richness,
        "morphological_disparity": round(disparity, 3),
        "age_range_ma": age_range,
        "key_genera": list(set(f['genus'] for f in fossils if f.get('genus')))
    }


def main():
    parser = argparse.ArgumentParser(
        description="Analyze Cambrian fossil assemblages for diversity and disparity."
    )
    parser.add_argument('--input', required=True, help='Input CSV file with fossil data')
    parser.add_argument('--output', required=True, help='Output JSON report file')
    args = parser.parse_args()

    try:
        fossils = load_fossils(args.input)
    except Exception as e:
        print(f"Error loading input: {e}", file=sys.stderr)
        sys.exit(1)

    report = analyze_site(fossils)
    Path(args.output).write_text(json.dumps(report, indent=2))
    print(f"Analysis complete. Report saved to {args.output}")


if __name__ == '__main__':
    main()
