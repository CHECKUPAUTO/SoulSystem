//! Genetic programming for symbolic expression evolution.
//!
//! Evolves `scirust_symbolic::Expr` trees using crossover and mutation.

use crate::core::Rng;
use scirust_symbolic::Expr;
use std::collections::HashMap;

/// A population of symbolic expressions.
pub struct SymbolicPopulation {
    pub expressions: Vec<Expr>,
    pub fitness: Vec<f64>,
    depth_limit: usize,
    variables: Vec<String>,
    num_constants: usize,
    rng: Rng,
}

impl SymbolicPopulation {
    pub fn new(size: usize, depth_limit: usize) -> Self {
        Self {
            expressions: Vec::with_capacity(size),
            fitness: Vec::with_capacity(size),
            depth_limit,
            variables: Vec::new(),
            num_constants: 0,
            rng: Rng::new(),
        }
    }

    pub fn initialize(&mut self, variables: &[&str], num_constants: usize) {
        self.variables = variables.iter().map(|s| s.to_string()).collect();
        self.num_constants = num_constants;
        while self.expressions.len() < self.expressions.capacity() {
            let expr = self.random_tree(3);
            self.expressions.push(expr);
            self.fitness.push(0.0);
        }
    }

    fn random_tree(&mut self, depth: usize) -> Expr {
        if depth == 0 || self.rng.next_f64() < 0.3 {
            if self.rng.next_f64() < 0.5 && !self.variables.is_empty() {
                let idx = self.rng.usize_range(0, self.variables.len());
                Expr::Var(self.variables[idx].clone())
            } else {
                let const_idx = self.rng.usize_range(0, self.num_constants + 1);
                Expr::Const(const_idx as f64 * 2.0 - 1.0)
            }
        } else {
            let left = Box::new(self.random_tree(depth - 1));
            let right = Box::new(self.random_tree(depth - 1));

            match self.rng.usize_range(0, 6) {
                0 => Expr::Add(left, right),
                1 => Expr::Sub(left, right),
                2 => Expr::Mul(left, right),
                3 => Expr::Div(left, right),
                4 => Expr::Sin(left),
                5 => Expr::Pow(left, right),
                _ => Expr::Var("x".to_string()),
            }
        }
    }

    pub fn crossover(&mut self, parent_a: &Expr, parent_b: &Expr) -> Expr {
        let nodes_a = collect_nodes(parent_a);
        let nodes_b = collect_nodes(parent_b);
        if nodes_a.is_empty() || nodes_b.is_empty() {
            return parent_a.clone();
        }
        let idx_a = self.rng.usize_range(0, nodes_a.len());
        let idx_b = self.rng.usize_range(0, nodes_b.len());
        replace_subtree(parent_a, idx_a, &nodes_b[idx_b])
    }

    pub fn mutate(&mut self, expr: &Expr) -> Expr {
        let nodes = collect_nodes(expr);
        if nodes.is_empty() { return expr.clone(); }
        let idx = self.rng.usize_range(0, nodes.len());
        let new_subtree = self.random_tree(2);
        replace_subtree(expr, idx, &new_subtree)
    }

    pub fn evolve<F: Fn(&Expr) -> f64>(&mut self, generations: usize, fitness_fn: &F) -> Expr {
        for _gen in 0..generations {
            for (i, expr) in self.expressions.iter().enumerate() {
                self.fitness[i] = fitness_fn(expr);
            }

            let mut indices: Vec<usize> = (0..self.expressions.len()).collect();
            indices.sort_by(|&a, &b| self.fitness[b].total_cmp(&self.fitness[a]));

            let mut new_exprs = Vec::with_capacity(self.expressions.len());
            new_exprs.push(self.expressions[indices[0]].clone());
            new_exprs.push(self.expressions[indices[1]].clone());

            while new_exprs.len() < self.expressions.len() {
                let a_idx = indices[self.rng.usize_range(0, 5.min(indices.len()))];
                let b_idx = indices[self.rng.usize_range(0, 5.min(indices.len()))];
                let (ea, eb) = (self.expressions[a_idx].clone(), self.expressions[b_idx].clone());
                let mut child = self.crossover(&ea, &eb);
                if self.rng.next_f64() < 0.1 {
                    child = self.mutate(&child);
                }
                new_exprs.push(child);
            }
            self.expressions = new_exprs;
        }

        for (i, expr) in self.expressions.iter().enumerate() {
            self.fitness[i] = fitness_fn(expr);
        }
        let best_idx = (0..self.expressions.len())
            .max_by(|&a, &b| self.fitness[a].total_cmp(&self.fitness[b]))
            .unwrap_or(0);
        self.expressions[best_idx].clone()
    }
}

fn collect_nodes(expr: &Expr) -> Vec<Expr> {
    let mut nodes = Vec::new();
    collect_nodes_rec(expr, &mut nodes);
    nodes
}

fn collect_nodes_rec(expr: &Expr, nodes: &mut Vec<Expr>) {
    nodes.push(expr.clone());
    match expr {
        Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b) | Expr::Div(a, b) | Expr::Pow(a, b) => {
            collect_nodes_rec(a, nodes);
            collect_nodes_rec(b, nodes);
        }
        Expr::Neg(a) | Expr::Sin(a) | Expr::Cos(a) | Expr::Exp(a) | Expr::Ln(a) | Expr::Sqrt(a) | Expr::Abs(a) => {
            collect_nodes_rec(a, nodes);
        }
        _ => {}
    }
}

fn replace_subtree(expr: &Expr, target_idx: usize, replacement: &Expr) -> Expr {
    let mut count = 0usize;
    replace_subtree_rec(expr, target_idx, replacement, &mut count)
}

fn replace_subtree_rec(expr: &Expr, target_idx: usize, replacement: &Expr, count: &mut usize) -> Expr {
    if *count == target_idx {
        *count += 1;
        return replacement.clone();
    }
    *count += 1;

    match expr {
        Expr::Add(a, b) => Expr::Add(
            Box::new(replace_subtree_rec(a, target_idx, replacement, count)),
            Box::new(replace_subtree_rec(b, target_idx, replacement, count)),
        ),
        Expr::Sub(a, b) => Expr::Sub(
            Box::new(replace_subtree_rec(a, target_idx, replacement, count)),
            Box::new(replace_subtree_rec(b, target_idx, replacement, count)),
        ),
        Expr::Mul(a, b) => Expr::Mul(
            Box::new(replace_subtree_rec(a, target_idx, replacement, count)),
            Box::new(replace_subtree_rec(b, target_idx, replacement, count)),
        ),
        Expr::Div(a, b) => Expr::Div(
            Box::new(replace_subtree_rec(a, target_idx, replacement, count)),
            Box::new(replace_subtree_rec(b, target_idx, replacement, count)),
        ),
        Expr::Pow(a, b) => Expr::Pow(
            Box::new(replace_subtree_rec(a, target_idx, replacement, count)),
            Box::new(replace_subtree_rec(b, target_idx, replacement, count)),
        ),
        Expr::Neg(a) | Expr::Sin(a) | Expr::Cos(a) | Expr::Exp(a) | Expr::Ln(a) | Expr::Sqrt(a) | Expr::Abs(a) => {
            let inner = replace_subtree_rec(a, target_idx, replacement, count);
            match expr {
                Expr::Neg(_) => Expr::Neg(Box::new(inner)),
                Expr::Sin(_) => Expr::Sin(Box::new(inner)),
                Expr::Cos(_) => Expr::Cos(Box::new(inner)),
                Expr::Exp(_) => Expr::Exp(Box::new(inner)),
                Expr::Ln(_) => Expr::Ln(Box::new(inner)),
                Expr::Sqrt(_) => Expr::Sqrt(Box::new(inner)),
                Expr::Abs(_) => Expr::Abs(Box::new(inner)),
                _ => unreachable!(),
            }
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scirust_symbolic::{parse, eval};

    #[test]
    fn test_collect_nodes() {
        let expr = parse("x + y * 2").unwrap();
        let nodes = collect_nodes(&expr);
        assert!(nodes.len() > 1);
    }

    #[test]
    fn test_replace_subtree() {
        let expr = parse("x + y").unwrap();
        let replacement = parse("42").unwrap();
        let result = replace_subtree(&expr, 2, &replacement);
        assert_eq!(format!("{}", result), "(x + 42)");
    }

    #[test]
    fn test_symbolic_regression() {
        let mut pop = SymbolicPopulation::new(50, 4);
        pop.initialize(&["x"], 3);

        let target = |x: f64| x * x + 1.0;

        let best = pop.evolve(10, &|expr| {
            let mut error = 0.0;
            for i in 0..20 {
                let x = i as f64 / 10.0 - 1.0;
                let mut vars = HashMap::new();
                vars.insert("x".to_string(), x);
                if let Ok(val) = eval(expr, &vars) {
                    error -= (val - target(x)).abs();
                } else {
                    error -= 100.0;
                }
            }
            error
        });

        // Verify the best expression can be evaluated
        let result = eval(&best, &HashMap::from([("x".to_string(), 2.0)]));
        assert!(result.is_ok(), "Best expression should be evaluable: {}", best);
    }
}
