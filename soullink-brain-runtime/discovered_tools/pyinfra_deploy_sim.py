#!/usr/bin/env python3
"""
Pyinfra Deploy Simulator

Simule un déploiement pyinfra en mode dry-run pour valider la syntaxe des opérations.
Basé sur l'architecture de pyinfra v3.8.0, sans dépendances externes.
"""

import argparse
import json
import sys
from typing import Any, Dict, List, Optional


class Operation:
    """Représente une opération pyinfra simulée."""
    def __init__(self, name: str, **kwargs):
        self.name = name
        self.kwargs = kwargs

    def execute(self, dry_run: bool = True) -> Dict[str, Any]:
        """Simule l'exécution de l'opération."""
        if dry_run:
            return {
                "operation": self.name,
                "dry_run": True,
                "status": "would_execute",
                "args": self.kwargs
            }
        else:
            return {
                "operation": self.name,
                "status": "executed",
                "args": self.kwargs
            }


class Deploy:
    """Simule un déploiement pyinfra."""
    def __init__(self, name: str = "default"):
        self.name = name
        self.operations: List[Operation] = []

    def add_operation(self, op: Operation):
        """Ajoute une opération au déploiement."""
        self.operations.append(op)

    def run(self, dry_run: bool = True) -> List[Dict[str, Any]]:
        """Exécute (ou simule) toutes les opérations."""
        results = []
        for op in self.operations:
            result = op.execute(dry_run=dry_run)
            results.append(result)
        return results


def main():
    parser = argparse.ArgumentParser(
        description="Simule un déploiement pyinfra en mode dry-run."
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        default=True,
        help="Mode simulation (par défaut)"
    )
    parser.add_argument(
        "--real",
        action="store_true",
        help="Mode exécution réelle (non implémenté ici)"
    )
    parser.add_argument(
        "--output",
        type=str,
        default="-",
        help="Fichier de sortie (par défaut stdout)"
    )
    args = parser.parse_args()

    if args.real:
        print("⚠️  Mode exécution réelle non implémenté dans cette simulation.", file=sys.stderr)
        sys.exit(1)

    deploy = Deploy(name="demo_deploy")
    deploy.add_operation(Operation("file.download", src="https://example.com/config.yaml", dest="/etc/app/config.yaml"))
    deploy.add_operation(Operation("server.service", name="nginx", running=True, enabled=True))
    deploy.add_operation(Operation("pip.packages", packages=["requests"], state="present"))

    results = deploy.run(dry_run=not args.real)

    output = json.dumps(results, indent=2)
    if args.output == "-":
        print(output)
    else:
        with open(args.output, "w") as f:
            f.write(output)


if __name__ == "__main__":
    main()
