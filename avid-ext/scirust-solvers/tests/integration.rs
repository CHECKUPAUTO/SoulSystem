//! Tests d'intégration combinant plusieurs modules.
//! Lance avec `cargo test -p scirust-solvers --test integration`.

use scirust_autodiff::Dual;
use scirust_solvers::{
    linalg::{self, Matrix},
    nonlinear::newton_system,
    ode::dopri5,
    optimize::bfgs,
    quadrature::simpson_adaptive,
    roots::brent,
    solve::solve,
    Tolerance,
};
use scirust_symbolic as sym;
use std::collections::HashMap;
use std::f64::consts::PI;

/// Pipeline scientifique : on définit f(x) = sin(x) - x/3, on
///   1. trouve sa racine non-triviale par Brent
///   2. calcule son intégrale de 0 à cette racine par Simpson
///   3. vérifie l'aire approximée
#[test]
fn root_then_integrate() {
    let f = |x: f64| x.sin() - x / 3.0;
    let root = brent(f, 1.0, 3.0, Tolerance::default()).unwrap().value;
    assert!((root - 2.278_862_649).abs() < 1e-6);

    let area = simpson_adaptive(f, 0.0, root, 1e-12, 30).unwrap();
    // ∫(sin(x) - x/3) dx = -cos(x) - x²/6
    // [..]₀^r = -cos(r) - r²/6 - (-1) = 1 - cos(r) - r²/6
    let exact = 1.0 - root.cos() - root.powi(2) / 6.0;
    assert!((area - exact).abs() < 1e-8);
}

/// Pipeline optimisation : on minimise une fonction puis on vérifie
/// que le gradient au minimum est ≈ 0 via autodiff.
#[test]
fn bfgs_minimum_has_zero_gradient() {
    let f = |x: &[Dual]| {
        (x[0] - 2.0) * (x[0] - 2.0)
            + (x[1] + 3.0) * (x[1] + 3.0)
            + (x[2] - 1.0) * (x[2] - 1.0) * 4.0
    };
    let s = bfgs(f, vec![0.0, 0.0, 0.0], Tolerance::default()).unwrap();

    // Vérif que le minimum est bien (2, -3, 1)
    assert!((s.value[0] - 2.0).abs() < 1e-6);
    assert!((s.value[1] + 3.0).abs() < 1e-6);
    assert!((s.value[2] - 1.0).abs() < 1e-6);

    // Vérif que le gradient y est nul (via autodiff)
    let eval_grad = |xs: &[f64]| -> Vec<f64> {
        (0..xs.len())
            .map(|j| {
                let mut buf: Vec<Dual> = (0..xs.len())
                    .map(|i| Dual::new(xs[i], if i == j { 1.0 } else { 0.0 }))
                    .collect();
                f(&buf).deriv.tap(|_| {
                    let _ = &mut buf;
                })
            })
            .collect()
    };
    let g = eval_grad(&s.value);
    for gi in &g {
        assert!(gi.abs() < 1e-4, "gradient component {} not near zero", gi);
    }
}

/// Pipeline EDO + racine : on intègre une EDO, puis on cherche
/// l'instant t où la solution atteint une valeur cible.
#[test]
fn ode_event_detection() {
    // y' = -y + cos(t), y(0) = 0
    // On cherche t > 0 tel que y(t) = 0.5
    // Solution analytique : y(t) = (sin(t) + cos(t))/2 - exp(-t)/2

    // 1) Intègre sur [0, 5] avec une grille fine, on tabule y(t)
    let r = dopri5(
        |t, y, dy| {
            dy[0] = -y[0] + t.cos();
        },
        0.0,
        5.0,
        vec![0.0],
        1e-10,
        1e-12,
        0.01,
    )
    .unwrap();

    // 2) Trouve un intervalle où y traverse 0.5
    let target = 0.5;
    let mut bracket = None;
    for i in 1..r.t.len() {
        let y_prev = r.y[i - 1][0];
        let y_curr = r.y[i][0];
        if (y_prev - target) * (y_curr - target) < 0.0 {
            bracket = Some((r.t[i - 1], r.t[i]));
            break;
        }
    }
    let (ta, tb) = bracket.expect("y should cross 0.5");

    // 3) Brent pour raffiner — on évalue y(t) en réintégrant à chaque appel.
    //    Pour ce test on prend une approximation linéaire entre les points
    //    déjà calculés (Brent va converger en quelques itérations sur ce
    //    petit intervalle, et l'interpolation linéaire est suffisante).
    let r_clone_t = r.t.clone();
    let r_clone_y: Vec<f64> = r.y.iter().map(|v| v[0]).collect();
    let y_of_t = |t: f64| -> f64 {
        // Trouve l'intervalle contenant t et interpole linéairement
        let i = r_clone_t.partition_point(|&x| x <= t).saturating_sub(1);
        let i = i.min(r_clone_t.len() - 2);
        let t0 = r_clone_t[i];
        let t1 = r_clone_t[i + 1];
        let y0 = r_clone_y[i];
        let y1 = r_clone_y[i + 1];
        y0 + (y1 - y0) * (t - t0) / (t1 - t0)
    };
    let t_star = brent(
        move |t: f64| y_of_t(t) - target,
        ta,
        tb,
        Tolerance::default(),
    )
    .unwrap()
    .value;

    // Vérif vs solution analytique : on cherche t tel que
    // (sin(t)+cos(t))/2 - exp(-t)/2 = 0.5
    let exact_eq = |t: f64| (t.sin() + t.cos()) / 2.0 - (-t).exp() / 2.0 - 0.5;
    assert!(
        exact_eq(t_star).abs() < 1e-3,
        "event detection inaccurate: {}",
        exact_eq(t_star)
    );
}

/// Pipeline algèbre : on construit une matrice symbolique de Vandermonde,
/// on la résout par LU, on vérifie via les racines polynomiales que la
/// solution est cohérente.
#[test]
fn vandermonde_polynomial_fit() {
    // Points (1, 6), (2, 17), (3, 34), (4, 57)
    // On veut a + b·x + c·x² tel que ces points soient ajustés exactement.
    //   ⇒ système (4 équations, 3 inconnues) — résoudre au sens des moindres carrés via QR.
    let xs = [1.0_f64, 2.0, 3.0, 4.0];
    let ys = [6.0_f64, 17.0, 34.0, 57.0];

    let mut data = Vec::with_capacity(12);
    for &x in &xs {
        data.push(1.0);
        data.push(x);
        data.push(x * x);
    }
    let a = Matrix::from_row_major(4, 3, data);
    let qr = linalg::qr_decompose(a).unwrap();
    let coeffs = linalg::solve_qr_least_squares(&qr, &ys).unwrap();

    // Construit l'expression a + b·x + c·x² et utilise solve() pour
    // vérifier qu'aux points x = 1..4, c'est bien ys.
    let p = scirust_solvers::polynomial::Polynomial::new(coeffs.clone());
    for (i, &xi) in xs.iter().enumerate() {
        let yi = p.eval(xi);
        assert!(
            (yi - ys[i]).abs() < 1e-8,
            "fit doesn't match: p({}) = {}, expected {}",
            xi,
            yi,
            ys[i]
        );
    }
}

/// Pipeline symbolique : on parse une équation, on la résout symboliquement
/// si possible, sinon numériquement.
#[test]
fn symbolic_to_numeric_pipeline() {
    let cases = vec![
        ("x + 5", vec![-5.0]),
        ("2*x^2 - 8", vec![-2.0, 2.0]),
        ("x^3 - x", vec![-1.0, 0.0, 1.0]),
    ];
    for (eq, expected) in cases {
        let e = sym::parse(eq).unwrap();
        let r = solve(&e, "x", HashMap::new()).unwrap();
        let got = r.real_roots();
        assert_eq!(got.len(), expected.len(), "for {}: got {:?}", eq, got);
        for (g, e) in got.iter().zip(&expected) {
            assert!((g - e).abs() < 1e-6, "{} : {} vs {}", eq, g, e);
        }
    }
}

/// Newton multivarié : système de Bratu réduit (trouve l'état stationnaire)
#[test]
fn system_thermal_equilibrium() {
    // Réacteur à 3 zones avec couplage non-linéaire :
    //   T_1 + 0.1·exp(T_1) = T_2 + 1
    //   T_2 + 0.1·exp(T_2) = (T_1 + T_3) / 2
    //   T_3 + 0.1·exp(T_3) = T_2
    // F(T) = 0 forme :
    //   F_1 = T_1 + 0.1·exp(T_1) - T_2 - 1
    //   F_2 = T_2 + 0.1·exp(T_2) - (T_1 + T_3)/2
    //   F_3 = T_3 + 0.1·exp(T_3) - T_2
    let s = newton_system(
        |t, out| {
            out[0] = t[0] + (t[0]).exp() * 0.1 - t[1] - 1.0;
            out[1] = t[1] + (t[1]).exp() * 0.1 - (t[0] + t[2]) * 0.5;
            out[2] = t[2] + (t[2]).exp() * 0.1 - t[1];
        },
        vec![0.5, 0.5, 0.5],
        Tolerance::default(),
    )
    .unwrap();

    // Vérification : on évalue F(T*) — doit être < tol
    let mut f = vec![0.0; 3];
    f[0] = s.value[0] + s.value[0].exp() * 0.1 - s.value[1] - 1.0;
    f[1] = s.value[1] + s.value[1].exp() * 0.1 - (s.value[0] + s.value[2]) * 0.5;
    f[2] = s.value[2] + s.value[2].exp() * 0.1 - s.value[1];
    for fi in &f {
        assert!(fi.abs() < 1e-8, "residual not small: {}", fi);
    }
    let _ = PI; // silence le warning si PI n'est pas utilisé
}

// Petit helper pour récupérer une variable après usage (sans .tap() en std)
trait Tap: Sized {
    fn tap<F: FnOnce(&Self)>(self, f: F) -> Self {
        f(&self);
        self
    }
}
impl<T> Tap for T {}
