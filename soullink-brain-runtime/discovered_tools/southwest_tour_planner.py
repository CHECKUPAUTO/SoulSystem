#!/usr/bin/env python3
"""
Southwest Headquarters Tour Planner

Démonstration d'un algorithme de planification de visite optimale inspiré de la logistique aérienne
utilisant une approximation du problème du voyageur de commerce (TSP) avec contraintes réelles.

Basé sur l'idée du paper : "Southwest Headquarters Tour 2026" (non disponible, mais inspiré des
pratiques d'optimisation logistique de Southwest Airlines).
"""

import argparse
import math
from itertools import permutations
from typing import List, Tuple, Dict


def haversine_distance(coord1: Tuple[float, float], coord2: Tuple[float, float]) -> float:
    """Calculate the great-circle distance between two points on Earth (in km)."""
    R = 6371  # Earth radius in km
    lat1, lon1 = math.radians(coord1[0]), math.radians(coord1[1])
    lat2, lon2 = math.radians(coord2[0]), math.radians(coord2[1])
    dlat = lat2 - lat1
    dlon = lon2 - lon1
    a = math.sin(dlat / 2) ** 2 + math.cos(lat1) * math.cos(lat2) * math.sin(dlon / 2) ** 2
    c = 2 * math.asin(math.sqrt(a))
    return R * c


def compute_total_route_distance(coords: List[Tuple[float, float]], order: List[int]) -> float:
    """Compute total distance for a given visit order (including return to start)."""
    total = 0.0
    for i in range(len(order)):
        total += haversine_distance(coords[order[i]], coords[order[(i + 1) % len(order)]])
    return total


def find_optimal_route(coords: List[Tuple[float, float]]) -> List[int]:
    """Find the shortest Hamiltonian cycle using brute-force for ≤9 points."""
    n = len(coords)
    if n <= 1:
        return [0]
    if n > 9:
        raise ValueError("Brute-force TSP solver supports up to 9 locations.")
    
    best_order = list(range(n))
    best_dist = compute_total_route_distance(coords, best_order)
    
    for perm in permutations(range(1, n)):
        order = [0] + list(perm)
        dist = compute_total_route_distance(coords, order)
        if dist < best_dist:
            best_dist = dist
            best_order = order
    return best_order


def main():
    parser = argparse.ArgumentParser(
        description="Planifie une visite optimale des sites Southwest à Dallas."
    )
    parser.add_argument(
        "--start",
        type=str,
        default="Love Field",
        help="Point de départ (ex: 'Love Field', 'HQ')"
    )
    parser.add_argument(
        "--locations",
        nargs="+",
        default=["Love Field", "HQ", "Maintenance Base"],
        help="Liste des sites à visiter (ex: --locations Love\u200bField HQ MaintenanceBase)"
    )
    args = parser.parse_args()

    # Coordinates (lat, lon) for Southwest sites in Dallas
    LOCATIONS_COORDS: Dict[str, Tuple[float, float]] = {
        "Love Field": (32.9696, -96.8525),
        "HQ": (32.9696, -96.8525),  # Southwest HQ is near Love Field
        "Maintenance Base": (32.9696, -96.8525),  # Simplified for demo
        "Dallas Love Field Airport": (32.9696, -96.8525),
        "Dallas/Fort Worth Airport": (32.8998, -97.0403),
    }

    # Resolve location names to coordinates
    coords = []
    for loc in args.locations:
        if loc in LOCATIONS_COORDS:
            coords.append(LOCATIONS_COORDS[loc])
        else:
            raise ValueError(f"Unknown location: {loc}")

    if not coords:
        parser.error("At least one location required.")

    # Find optimal route
    order = find_optimal_route(coords)
    
    # Output result
    print("Optimal Southwest Tour Route:")
    for i, idx in enumerate(order):
        loc_name = [k for k, v in LOCATIONS_COORDS.items() if v == coords[idx]][0]
        print(f"  {i+1}. {loc_name}")
    print(f"\nTotal estimated distance: {compute_total_route_distance(coords, order):.2f} km")


if __name__ == "__main__":
    main()