#!/usr/bin/env python3
"""
ArXiv Paper Summarizer - Inspired by arXiv:2605.00634v1

This tool generates a structured summary of an arXiv paper by extracting
key sentences from each major section (Introduction, Method, Results, Conclusion)
using a lightweight heuristic based on sentence position and lexical density.
"""

import argparse
import re
import sys
from typing import List, Tuple


def extract_sections(text: str) -> dict:
    """Extract sections from raw text using section headers."""
    sections = {}
    pattern = r"(?i)^\s*(introduction|method|methods|results|conclusion|discussion)\s*\n"
    blocks = re.split(pattern, text, flags=re.MULTILINE)
    
    if len(blocks) < 2:
        return {"default": text}
    
    # First block is before first section header
    current_section = "introduction" if blocks[0].strip() else None
    
    for i, block in enumerate(blocks[1:], 1):
        if i % 2 == 1:  # Section name
            current_section = block.strip().lower()
        else:  # Section content
            if current_section:
                sections[current_section] = block.strip()
    
    # Fallback: put everything in default if no sections found
    if not sections:
        sections["default"] = text.strip()
    
    return sections


def score_sentence(sentence: str) -> float:
    """Score a sentence based on lexical density and length."""
    words = [w for w in sentence.split() if len(w) > 2]
    if not words:
        return 0.0
    
    # Lexical density: ratio of content words to total words
    content_words = sum(1 for w in words if w[0].isupper() or w.isalpha())
    density = content_words / len(words)
    
    # Length penalty: avoid very short or very long sentences
    length_factor = min(len(words) / 10, 1.0)
    
    return density * length_factor


def extract_summary_sentences(text: str, max_sentences: int = 3) -> List[str]:
    """Extract top sentences from a text block."""
    sentences = re.split(r'(?<=[.!?])\s+', text.strip())
    scored = [(s, score_sentence(s)) for s in sentences if len(s.split()) > 5]
    scored.sort(key=lambda x: x[1], reverse=True)
    return [s for s, _ in scored[:max_sentences]]


def generate_summary(text: str) -> str:
    """Generate a structured summary from paper text."""
    sections = extract_sections(text)
    summary_parts = []
    
    for section in ["introduction", "method", "results", "conclusion", "default"]:
        if section in sections:
            sentences = extract_summary_sentences(sections[section], max_sentences=2)
            if sentences:
                summary_parts.append(f"**{section.capitalize()}**")
                summary_parts.extend(f"- {s}" for s in sentences)
    
    return "\n".join(summary_parts) if summary_parts else "No summary could be generated."


def main():
    parser = argparse.ArgumentParser(
        description="Generate structured summaries for arXiv papers."
    )
    parser.add_argument("input_file", nargs="?", help="Path to paper text file")
    parser.add_argument("--text", type=str, help="Raw paper text as a string")
    args = parser.parse_args()
    
    if args.text:
        text = args.text
    elif args.input_file:
        with open(args.input_file, "r", encoding="utf-8") as f:
            text = f.read()
    else:
        # Read from stdin
        text = sys.stdin.read()
    
    if not text.strip():
        print("Error: No input provided.", file=sys.stderr)
        sys.exit(1)
    
    print(generate_summary(text))


if __name__ == "__main__":
    main()
