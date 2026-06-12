//! Démonstration complète de `scirust-solvers`.
//!
//! Lance avec : `cargo run --example showcase -p scirust-solvers`

use scirust_autodiff::Dual;
use scirust_solvers::{
    linalg::{self, Matrix},
    nonlinear::newton_system,
    ode::dopri5,
    optimize::{bfgs, nelder_mead},
    polynomial::Polynomial,
    quadrature::{gauss_legendre, simpson_adaptive, GaussOrder},
    roots::{brent, newton},
    solve::{solve, solve_in_interval},
    Tolerance,
};
use scirust_symbolic as sym;
use std::collections::HashMap;
use std::f64::consts::PI;

fn main() {
    banner("1. Recherche de racine 1D");
    demo_roots_1d();

    banner("2. Systèmes non-linéaires");
    demo_nonlinear_system();

    banner("3. Algèbre linéaire");
    demo_linalg();

    banner("4. Équations différentielles ordinaires");
    demo_ode();

    banner("5. Optimisation");
    demo_optimization();

    banner("6. Quadrature (intégration numérique)");
    demo_quadrature();

    banner("7. Racines de polynômes");
    demo_polynomial();

    banner("8. API unifiée symbolique → numérique");
    demo_unified_solve();
}

fn banner(title: &str) {
    println!("\n══════════════════════════════════════════════════════════════════════");
    println!("  {}", title);
    println!("══════════════════════════════════════════════════════════════════════");
}

fn demo_roots_1d() {
    // Méthode 1 : Brent (sûr, rapide, recommandé par défaut)
    let s = brent(|x| x.cos(), 0.0, PI, Tolerance::default()).unwrap();
    println!(
        "Brent      | cos(x) = 0 dans [0,π]          → x = {:.15} ({} itér.)",
        s.value, s.info.iterations
    );

    // Méthode 2 : Newton avec dérivée automatique (dual numbers)
    let s = newton(
        |x: Dual| x.powi(3) - x * 2.0 - 5.0,
        2.0,
        Tolerance::default(),
    )
    .unwrap();
    println!(
        "Newton(AD) | x³-2x-5 = 0 (autodiff exact)   → x = {:.15} ({} itér.)",
        s.value, s.info.iterations
    );
}

fn demo_nonlinear_system() {
    // Système : x² + y² = 1, x = y  →  (1/√2, 1/√2)
    let s = newton_system(
        |x, out| {
            out[0] = x[0] * x[0] + x[1] * x[1] - 1.0;
            out[1] = x[0] - x[1];
        },
        vec![1.0, 0.5],
        Tolerance::default(),
    )
    .unwrap();
    println!(
        "Newton 2D  | x²+y²=1, x=y                   → ({:.10}, {:.10}) ({} itér.)",
        s.value[0], s.value[1], s.info.iterations
    );
}

fn demo_linalg() {
    // Système 3×3 : Ax = b
    let a = Matrix::from_row_major(
        3,
        3,
        vec![2.0, 1.0, -1.0, -3.0, -1.0, 2.0, -2.0, 1.0, 2.0],
    );
    let b = vec![8.0, -11.0, -3.0];
    let x = linalg::solve(a.clone(), &b).unwrap();
    println!(
        "LU         | A·x = b  (3×3)                 → x = [{:.6}, {:.6}, {:.6}]",
        x[0], x[1], x[2]
    );
    println!("           | det(A) = {:.6}", a.determinant().unwrap());
    let inv = a.inverse().unwrap();
    println!(
        "           | A⁻¹ vérifiée par ||A·A⁻¹ - I|| = {:.3e}",
        {
            let p = a.matmul(&inv).unwrap();
            let id = Matrix::identity(3);
            let mut e = 0.0_f64;
            for i in 0..3 {
                for j in 0..3 {
                    e = e.max((p[(i, j)] - id[(i, j)]).abs());
                }
            }
            e
        }
    );
}

fn demo_ode() {
    // Oscillateur harmonique : θ'' = -θ, intégré sur [0, 2π]
    // En t = 2π, θ doit retrouver θ(0).
    let r = dopri5(
        |_t, y, dy| {
            dy[0] = y[1];
            dy[1] = -y[0];
        },
        0.0,
        2.0 * PI,
        vec![1.0, 0.0],
        1e-10,
        1e-12,
        0.1,
    )
    .unwrap();
    let final_y = r.y.last().unwrap();
    println!(
        "DOPRI5     | θ''=-θ sur [0,2π]              → θ(2π) = {:.12} (devrait être 1)",
        final_y[0]
    );
    println!(
        "           | {} pas acceptés, {} rejetés",
        r.accepted, r.rejected
    );
}

fn demo_optimization() {
    // Fonction de Rosenbrock : f(x,y) = (1-x)² + 100(y-x²)², minimum en (1,1)
    let f = |x: &[Dual]| {
        let a = -x[0] + 1.0;
        let b = x[1] - x[0] * x[0];
        a * a + b * b * 100.0
    };
    let s = bfgs(f, vec![-1.2, 1.0], Tolerance::default()).unwrap();
    println!(
        "BFGS       | Rosenbrock                     → ({:.10}, {:.10}) ({} itér.)",
        s.value[0], s.value[1], s.info.iterations
    );

    // Nelder-Mead sur une fonction non-différentiable
    let s = nelder_mead(
        |x| (x[0] - 2.0).abs() + (x[1] + 1.0).abs(),
        vec![0.0, 0.0],
        1.0,
        Tolerance::default(),
    )
    .unwrap();
    println!(
        "Nelder-Mead| min |x-2| + |y+1|              → ({:.4}, {:.4}) ({} itér.)",
        s.value[0], s.value[1], s.info.iterations
    );
}

fn demo_quadrature() {
    // ∫₀^π sin(x) dx = 2
    let v = simpson_adaptive(|x| x.sin(), 0.0, PI, 1e-12, 30).unwrap();
    println!("Simpson    | ∫₀^π sin(x) dx                 → {:.15} (exact = 2)", v);

    // Gauss-Legendre ordre 20 sur l'intégrale de Runge ∫₋₅⁵ 1/(1+x²) dx = 2·atan(5)
    let v = gauss_legendre(|x: f64| 1.0 / (1.0 + x * x), -5.0, 5.0, GaussOrder::Twenty);
    let exact = 2.0 * 5.0_f64.atan();
    println!(
        "Gauss-Leg  | ∫₋₅⁵ 1/(1+x²) dx               → {:.12} (exact = {:.12})",
        v, exact
    );
}

fn demo_polynomial() {
    // p(x) = x³ - 6x² + 11x - 6 = (x-1)(x-2)(x-3)
    let p = Polynomial::from_descending(vec![1.0, -6.0, 11.0, -6.0]);
    let reals = p.real_roots(1e-6).unwrap();
    println!(
        "Poly       | x³ - 6x² + 11x - 6             → racines réelles {:?}",
        reals
    );

    // Wilkinson's polynomial (mal-conditionné classique)
    // p(x) = (x-1)(x-2)...(x-5)
    let mut q = Polynomial::from_descending(vec![1.0, -1.0]);
    for k in 2..=5 {
        let r = Polynomial::from_descending(vec![1.0, -(k as f64)]);
        let mut new_coeffs = vec![0.0; q.coeffs.len() + r.coeffs.len() - 1];
        for (i, &a) in q.coeffs.iter().enumerate() {
            for (j, &b) in r.coeffs.iter().enumerate() {
                new_coeffs[i + j] += a * b;
            }
        }
        q = Polynomial::new(new_coeffs);
    }
    let reals = q.real_roots(1e-5).unwrap();
    println!(
        "Poly d=5   | (x-1)(x-2)(x-3)(x-4)(x-5)      → {:?}",
        reals
    );
}

fn demo_unified_solve() {
    // Linéaire
    let e = sym::parse("2*x + 4").unwrap();
    let r = solve(&e, "x", HashMap::new()).unwrap();
    println!(
        "solve      | 2x + 4 = 0                     → x = {:?} (linéaire — exact)",
        r.real_roots()
    );

    // Quadratique → formule exacte
    let e = sym::parse("x^2 - 5*x + 6").unwrap();
    let r = solve(&e, "x", HashMap::new()).unwrap();
    println!(
        "solve      | x² - 5x + 6 = 0                → x = {:?} (quadratique — exact)",
        r.real_roots()
    );

    // Cubique → Durand-Kerner
    let e = sym::parse("x^3 - 6*x^2 + 11*x - 6").unwrap();
    let r = solve(&e, "x", HashMap::new()).unwrap();
    println!(
        "solve      | x³ - 6x² + 11x - 6 = 0         → x = {:?} (Durand-Kerner)",
        r.real_roots()
    );

    // Transcendant → Brent dans un intervalle
    let e = sym::parse("cos(x) - x/3").unwrap();
    let v = solve_in_interval(&e, "x", 0.0, 2.0, HashMap::new(), Tolerance::default()).unwrap();
    println!(
        "solve      | cos(x) - x/3 = 0 dans [0,2]    → x = {:.12} (Brent symbolique)",
        v
    );
}
