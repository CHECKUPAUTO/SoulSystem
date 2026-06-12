#!/usr/bin/env python3
"""
Text-to-CAD Primitive Generator

Démontre l'idée clé du paper 'Text-to-CAD' : convertir du texte en géométrie 2D.
Ici, on se concentre sur la reconnaissance de primitives simples via des mots-clés.
"""

import argparse
import re
from dataclasses import dataclass
from typing import List, Tuple, Optional


@dataclass
class Primitive:
    """Représentation d'une primitive géométrique."""
    name: str
    params: dict


def parse_text(text: str) -> List[Primitive]:
    """Analyse un texte et extrait les primitives demandées."""
    primitives = []
    text = text.lower()

    # Détection de cercle
    match = re.search(r"cercle\s+(?:de\s+)?rayon\s+(\d+(?:\.\d+)?)", text)
    if match:
        radius = float(match.group(1))
        primitives.append(Primitive("circle", {"radius": radius}))

    # Détection de rectangle
    match = re.search(r"rectangle\s+(?:de\s+)?dimensions\s+(\d+(?:\.\d+)?)\s*x\s*(\d+(?:\.\d+)?)", text)
    if match:
        width, height = float(match.group(1)), float(match.group(2))
        primitives.append(Primitive("rectangle", {"width": width, "height": height}))

    # Détection de triangle équilatéral
    match = re.search(r"triangle\s+(?:équilatéral\s+)?côté\s+(\d+(?:\.\d+)?)", text)
    if match:
        side = float(match.group(1))
        primitives.append(Primitive("equilateral_triangle", {"side": side}))

    return primitives


def generate_gcode(primitives: List[Primitive]) -> str:
    """Génère du G-code simple pour les primitives."""
    lines = ["G21 (Unités en mm)", "G90 (Mode absolu)"]
    for i, p in enumerate(primitives):
        if p.name == "circle":
            r = p.params["radius"]
            lines.extend([
                f"(Cercle rayon {r})",
                "G0 X0 Y0",
                "G2 X0 Y0 I0 J" + f"{r:.3f}"
            ])
        elif p.name == "rectangle":
            w, h = p.params["width"], p.params["height"]
            lines.extend([
                f"(Rectangle {w}x{h})",
                "G0 X0 Y0",
                "G1 X" + f"{w:.3f}" + " Y0",
                "G1 X" + f"{w:.3f}" + " Y" + f"{h:.3f}",
                "G1 X0 Y" + f"{h:.3f}",
                "G1 X0 Y0"
            ])
        elif p.name == "equilateral_triangle":
            s = p.params["side"]
            h = s * (3 ** 0.5) / 2
            lines.extend([
                f"(Triangle équilatéral côté {s})",
                "G0 X0 Y0",
                "G1 X" + f"{s:.3f}" + " Y0",
                "G1 X" + f"{s/2:.3f}" + " Y" + f"{h:.3f}",
                "G1 X0 Y0"
            ])
    lines.append("M30 (Fin)")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description="Générateur de primitives CAD à partir de texte")
    parser.add_argument("--text", "-t", required=True, help="Description textuelle (ex: 'cercle de rayon 10 et rectangle de dimensions 20x15')")
    parser.add_argument("--output", "-o", default="output.nc", help="Fichier de sortie G-code (défaut: output.nc)")
    args = parser.parse_args()

    primitives = parse_text(args.text)
    if not primitives:
        print("Aucune primitive détectée. Essayez : 'cercle de rayon 5', 'rectangle de dimensions 10x20', 'triangle côté 8'")
        return

    gcode = generate_gcode(primitives)
    with open(args.output, "w") as f:
        f.write(gcode)
    print(f"G-code généré pour {len(primitives)} primitive(s) → {args.output}")


if __name__ == "__main__":
    main()