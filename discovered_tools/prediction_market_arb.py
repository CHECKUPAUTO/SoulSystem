#!/usr/bin/env python3
"""
Prediction Market Arbitrage Scanner.

Détecte les opportunités d'arbitrage entre deux marchés de prédiction
(Polymarket/Kalshi) en comparant les prix implicites et les marges de sécurité.

Basé sur l'idée centrale du WSJ paper : identifier les écarts de prix entre
marchés de prédiction causés par des liquidités inégales ou des biais cognitifs.
"""

import argparse
import sys
from typing import List, Tuple, Dict


def implied_probability(american_odds: int) -> float:
    """Convertit des cotes américaines en probabilité implicite."""
    if american_odds > 0:
        return 100 / (american_odds + 100)
    else:
        return abs(american_odds) / (abs(american_odds) + 100)


def find_arbitrage_opportunities(markets: List[Dict]) -> List[Tuple]:
    """
    Recherche des opportunités d'arbitrage entre les marchés.
    
    Un arbitrage existe si la somme des probabilités implicites pour un même événement
    est < 1 (après ajustement pour marge de sécurité). On applique une marge de sécurité
    de 5% pour compenser les frais de transaction et les risques.
    """
    opportunities = []
    margin_safety = 0.05  # 5% marge de sécurité
    
    for market in markets:
        prob_yes = implied_probability(market['odds_yes'])
        prob_no = implied_probability(market['odds_no'])
        total_prob = prob_yes + prob_no
        
        # Arbitrage possible si somme < 1 - marge
        if total_prob < 1 - margin_safety:
            arbitrage_margin = (1 - total_prob) / total_prob * 100
            opportunities.append({
                'event': market['event'],
                'prob_yes': prob_yes,
                'prob_no': prob_no,
                'total_prob': total_prob,
                'arbitrage_margin_pct': round(arbitrage_margin, 2)
            })
    
    return opportunities


def main():
    parser = argparse.ArgumentParser(
        description='Scanne les marchés de prédiction pour détecter des opportunités d\'arbitrage.'
    )
    parser.add_argument(
        '--odds_yes', type=int, required=True,
        help='Cote américaine pour le oui (ex: +150 ou -120)'
    )
    parser.add_argument(
        '--odds_no', type=int, required=True,
        help='Cote américaine pour le non (ex: +150 ou -120)'
    )
    parser.add_argument(
        '--event', type=str, default='Unknown',
        help='Nom de l\'événement (ex: "Fed Rate Decision")'
    )
    
    args = parser.parse_args()
    
    market = {
        'event': args.event,
        'odds_yes': args.odds_yes,
        'odds_no': args.odds_no
    }
    
    opportunities = find_arbitrage_opportunities([market])
    
    if not opportunities:
        print(f"Aucune opportunité d\'arbitrage détectée pour '{args.event}'.")
        sys.exit(0)
    
    opp = opportunities[0]
    print(f"Opportunité d\'arbitrage détectée pour '{opp['event']}'!")
    print(f"  Probabilité implicite OUI : {opp['prob_yes']*100:.2f}%")
    print(f"  Probabilité implicite NON : {opp['prob_no']*100:.2f}%")
    print(f"  Probabilité totale : {opp['total_prob']*100:.2f}%")
    print(f"  Marge d\'arbitrage (après marge de sécurité 5%) : {opp['arbitrage_margin_pct']:.2f}%")
    print("  → Stratégie : Achat proportionnel selon les probabilités inverses.")


if __name__ == '__main__':
    main()
