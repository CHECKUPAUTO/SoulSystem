#!/usr/bin/env python3
"""
Abstract Nonsense Demo: Contravariant Functor for Context Transformation

This script demonstrates the core idea from the paper: using contravariant
functors to model how transformations of input space induce transformations
of output functionals (e.g., pulling back functions along maps).
"""

import argparse
from typing import Callable, TypeVar, Generic, List

T = TypeVar('T')
U = TypeVar('U')
V = TypeVar('V')


class ContravariantFunctor(Generic[T]):
    """A simple contravariant functor representing functions T -> Any."""
    
    def __init__(self, value: Callable[[T], any]):
        self.value = value
    
    def contramap(self, f: Callable[[U], T]) -> 'ContravariantFunctor[U]':
        """Apply contravariant mapping: given f: U -> T, produce U -> Any."""
        return ContravariantFunctor(lambda u: self.value(f(u)))
    
    def evaluate(self, x: T) -> any:
        """Evaluate the contained function at point x."""
        return self.value(x)


def identity(x: T) -> T:
    """Identity function."""
    return x


def compose(f: Callable[[U], V], g: Callable[[T], U]) -> Callable[[T], V]:
    """Function composition: (f ∘ g)(x) = f(g(x))."""
    return lambda x: f(g(x))


def main():
    parser = argparse.ArgumentParser(
        description='Demonstrate contravariant functors in functional programming.'
    )
    parser.add_argument(
        '--input', type=str, default='hello',
        help='Input string to transform (default: hello)'
    )
    args = parser.parse_args()
    
    # Define a context: a function that measures string length
    length_functor = ContravariantFunctor(len)
    
    # Define a transformation: prepend 'pre_' to any string
    def prepend_context(s: str) -> str:
        return 'pre_' + s
    
    # Pull back the length function along prepend_context
    transformed = length_functor.contramap(prepend_context)
    
    # Evaluate on input
    result = transformed.evaluate(args.input)
    print(f"Original length of '{args.input}': {len(args.input)}")
    print(f"After prepending 'pre_': length = {result}")
    print(f"Verification: len('pre_{args.input}') = {len('pre_' + args.input)}")
    
    # Show functor laws
    print("\nFunctor laws check:")
    # Law 1: contramap(id) == id
    id_map = length_functor.contramap(identity)
    assert id_map.evaluate(args.input) == length_functor.evaluate(args.input)
    print("✓ Identity law holds")
    
    # Law 2: contramap(f ∘ g) == contramap(g) ∘ contramap(f)
    def double_upper(s: str) -> str:
        return s.upper() + s.upper()
    composed = length_functor.contramap(compose(double_upper, prepend_context))
    composed_alt = length_functor.contramap(prepend_context).contramap(double_upper)
    assert composed.evaluate(args.input) == composed_alt.evaluate(args.input)
    print("✓ Composition law holds")


if __name__ == '__main__':
    main()
