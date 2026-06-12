#!/usr/bin/env python3
"""
NPP Trademark Infringement Checker

Outil de détection préliminaire de violations de marque de Notepad++
basé sur les directives officielles publiées sur notepad-plus-plus.org.
"""

import argparse
import re
import sys


def check_trademark_infringement(text: str) -> dict:
    """Analyse un texte pour détecter des indices de violation de marque Notepad++.
    
    Args:
        text: Chaîne à analyser.
        
    Returns:
        Dictionnaire avec:
        - 'violations': liste des types de violations détectées
        - 'score': indicateur de risque (0-3)
        - 'details': messages explicatifs
    """
    violations = []
    details = []
    
    # Patterns pour détection de marques déposées
    npp_exact = re.compile(r'\bnotepad\+\+\b', re.IGNORECASE)
    npp_abbrev = re.compile(r'\bnpp\b', re.IGNORECASE)
    npp_with_suffix = re.compile(r'\bnotepad\s*\+\s*\+\s*[a-z]+\b', re.IGNORECASE)
    
    # Vérification des cas de violation
    if npp_exact.search(text):
        violations.append("exact_trademark_usage")
        details.append("Usage non modifié de 'Notepad++' (marque déposée)")
    
    if npp_abbrev.search(text):
        violations.append("abbreviated_trademark")
        details.append("Usage de l'abréviation 'npp' comme marque déposée")
    
    if npp_with_suffix.search(text):
        violations.append("derivative_name")
        details.append("Nom dérivé (ex: 'Notepad++ Pro') - interdit sans autorisation")
    
    # Score de risque
    score = min(3, len(violations))
    
    return {
        "violations": violations,
        "score": score,
        "details": details,
        "is_infringing": score >= 2
    }


def main():
    parser = argparse.ArgumentParser(
        description="Vérifie les violations potentielles de marque Notepad++ dans du texte."
    )
    parser.add_argument(
        "text",
        nargs="?",
        help="Texte à analyser (sinon lit depuis stdin)"
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Sortie en format JSON"
    )
    args = parser.parse_args()
    
    # Récupération du texte
    if args.text:
        text = args.text
    else:
        text = sys.stdin.read().strip()
    
    # Analyse
    result = check_trademark_infringement(text)
    
    # Sortie
    if args.json:
        import json
        print(json.dumps(result, indent=2))
    else:
        print(f"Score de risque: {result['score']}/3")
        if result['violations']:
            print("Violations détectées:")
            for detail in result['details']:
                print(f"  • {detail}")
        else:
            print("Aucune violation détectée.")
        if result['is_infringing']:
            print("⚠️  RECOMMANDATION: Consultez un avocat avant utilisation.")


if __name__ == "__main__":
    main()
