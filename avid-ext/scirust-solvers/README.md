# scirust-solvers

Résolution d'équations, EDO, optimisation et intégration en pur Rust, construit
au-dessus de [`scirust-autodiff`](../scirust-autodiff) pour les gradients
exacts et [`scirust-symbolic`](../scirust-symbolic) pour le chemin
symbolique → numérique.

## Couvre

| Catégorie | Méthodes |
|---|---|
| **Algèbre linéaire** | LU (pivot partiel), QR (Householder), Cholesky, déterminant, inverse, gradient conjugué |
| **Racines scalaires `f(x) = 0`** | Bissection, sécante, Newton (autodiff), Brent |
| **Systèmes non-linéaires `F(x) = 0`** | Newton-Raphson (jacobienne autodiff), Broyden quasi-Newton |
| **EDO `dy/dt = f(t,y)`** | RK4 pas fixe, Dormand-Prince 5(4) adaptatif |
| **Optimisation `min f(x)`** | Descente de gradient + line search, BFGS, Nelder-Mead |
| **Quadrature `∫f dx`** | Simpson adaptatif, Gauss-Legendre (ordres 5/10/20), Romberg |
| **Polynômes** | Évaluation Horner, dérivation formelle, racines (Durand-Kerner) |
| **API unifiée** | `solve(expr, var)` dispatch symbolique → numérique |

73 tests, tout passe.

## Exemple

```rust
use scirust_solvers::{roots::newton, Tolerance};
use scirust_autodiff::Dual;

// Une racine 1D avec dérivée exacte (autodiff)
let s = newton(|x: Dual| x.powi(3) - x * 2.0 - 5.0, 2.0, Tolerance::default())?;
println!("racine ≈ {}", s.value);  // 2.094551481542327
```

```rust
use scirust_solvers::solve::solve;
use scirust_symbolic as sym;
use std::collections::HashMap;

// API symbolique — détecte degré 2, formule exacte
let e = sym::parse("x^2 - 5*x + 6").unwrap();
let r = solve(&e, "x", HashMap::new())?;
println!("{:?}", r.real_roots());  // [2.0, 3.0]
```

```rust
use scirust_solvers::ode::dopri5;
// Pendule non-linéaire, intégrateur adaptatif
let r = dopri5(
    |_t, y, dy| { dy[0] = y[1]; dy[1] = -9.81 * y[0].sin(); },
    0.0, 10.0,
    vec![0.5, 0.0],
    1e-8, 1e-10, 0.05,
)?;
```

```rust
use scirust_solvers::optimize::bfgs;
use scirust_autodiff::Dual;
// Rosenbrock — converge en ~36 itérations
let s = bfgs(
    |x: &[Dual]| {
        let a = -x[0] + 1.0;
        let b = x[1] - x[0]*x[0];
        a*a + b*b*100.0
    },
    vec![-1.2, 1.0],
    Default::default(),
)?;
```

## Démo complète

```bash
cargo run --example showcase -p scirust-solvers --release
```

Affiche les 8 catégories de solveurs en action sur des cas tests classiques.

## Comment ça marche

### Gradients exacts par autodiff

La plupart des solveurs (Newton 1D, Newton système, gradient descent, BFGS)
demandent un gradient ou une jacobienne. On ne calcule **jamais** par
différences finies : on utilise `scirust_autodiff::Dual` pour avoir la
dérivée analytique-exacte au bit près en mode forward.

Conséquence : convergence quadratique propre (Newton), pas d'erreur de
troncature, pas de problème de choix du `h`.

### Dispatch symbolique → numérique

L'API `solve(expr, var)` analyse d'abord la structure de l'expression :

- Si elle se réduit à un polynôme de degré 1 ou 2 → **formule fermée**
- Si elle se réduit à un polynôme de degré ≥ 3 → **Durand-Kerner** (toutes les
  racines réelles ET complexes)
- Sinon → demande un intervalle (`solve_in_interval`) ou un point de départ
  (`solve_near`), puis **Brent** ou **Newton**

### Pas de dépendance externe lourde

Seul `thiserror` pour les erreurs typées, et `approx` pour les tests. Toute
l'algèbre linéaire est codée à la main (LU, QR, Cholesky) en row-major
contigu, sans BLAS. C'est volontaire : on garde la même philosophie que
`scirust-autodiff` et `scirust-simd` — minimal, lisible, hackable.

## Limitations connues

- **Mode forward seulement** pour l'autodiff dans les solveurs vectoriels :
  coûte N+1 évaluations par itération. Pour des problèmes à des centaines de
  variables, ajouter un backend reverse-mode (`Tape` est déjà dans
  `scirust-autodiff`, juste pas branché ici).
- **EDO raides** non couvertes : DOPRI5 est non-implicite. Pour les systèmes
  raides il faudrait ajouter BDF ou Rosenbrock.
- **Algèbre linéaire dense** uniquement. Pour des grosses matrices creuses,
  passer par `linalg::iterative::conjugate_gradient` (matrix-free).
- **Pas de symbolique** complète : la couche `scirust-symbolic` couvre
  parse / simplify / diff / solve linéaire et quadratique. Pour de
  l'intégration symbolique ou des systèmes algébriques exacts, il faut une
  CAS plus poussée.

## Roadmap

- BDF / Rosenbrock pour EDO raides
- L-BFGS (mémoire limitée, pour gros problèmes)
- Reverse-mode autodiff dans les solveurs multivariés (utiliser `Tape`)
- SVD (et donc pseudo-inverse et moindres carrés tronqués)
- Levenberg-Marquardt pour moindres carrés non-linéaires
- Méthodes de continuation / homotopie pour les systèmes difficiles
