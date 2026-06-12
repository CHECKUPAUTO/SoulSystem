use rustpython_ast::text_size::TextRange;
use rustpython_ast::{Stmt, Visitor};

/// Depth profile: (`max_depth`, `mean_depth`, `stdev_depth`) across function bodies.
pub type DepthProfile = (u32, f32, f32);

/// Collect depth profiles for all function bodies in the module.
pub fn collect_depth_profiles(suite: &[Stmt<TextRange>]) -> Vec<DepthProfile> {
    let mut collector = DepthCollector::default();
    for stmt in suite {
        collector.visit_stmt(stmt.clone());
    }
    collector.profiles
}

#[derive(Default)]
struct DepthCollector {
    profiles: Vec<DepthProfile>,
}

impl Visitor<TextRange> for DepthCollector {
    fn visit_stmt(&mut self, node: Stmt<TextRange>) {
        match &node {
            Stmt::FunctionDef(fd) => {
                let depths = collect_block_depths(&fd.body);
                if !depths.is_empty() {
                    self.profiles.push(compute_profile(&depths));
                }
                self.generic_visit_stmt(node);
                return;
            }
            Stmt::AsyncFunctionDef(fd) => {
                let depths = collect_block_depths(&fd.body);
                if !depths.is_empty() {
                    self.profiles.push(compute_profile(&depths));
                }
                self.generic_visit_stmt(node);
                return;
            }
            _ => {}
        }
        self.generic_visit_stmt(node);
    }
}

#[allow(clippy::cast_precision_loss)]
fn compute_profile(depths: &[u32]) -> DepthProfile {
    let max = *depths.iter().max().unwrap_or(&0);
    let n = depths.len() as f32;
    let mean = depths.iter().sum::<u32>() as f32 / n;
    let variance = depths
        .iter()
        .map(|&d| (d as f32 - mean).powi(2))
        .sum::<f32>()
        / n;
    (max, mean, variance.sqrt())
}

/// Collect nesting depths for all statements in a function body.
fn collect_block_depths(body: &[Stmt<TextRange>]) -> Vec<u32> {
    let mut depth_collector = DepthWalker {
        depths: Vec::new(),
        level: 0,
    };
    for stmt in body {
        depth_collector.visit_stmt(stmt.clone());
    }
    depth_collector.depths
}

struct DepthWalker {
    depths: Vec<u32>,
    level: u32,
}

impl Visitor<TextRange> for DepthWalker {
    fn visit_stmt(&mut self, node: Stmt<TextRange>) {
        self.depths.push(self.level);
        let was_block = matches!(
            &node,
            Stmt::If(_)
                | Stmt::For(_)
                | Stmt::AsyncFor(_)
                | Stmt::While(_)
                | Stmt::With(_)
                | Stmt::AsyncWith(_)
                | Stmt::Try(_)
                | Stmt::TryStar(_)
                | Stmt::FunctionDef(_)
                | Stmt::AsyncFunctionDef(_)
                | Stmt::Match(_)
        );
        if was_block {
            self.level += 1;
        }
        self.generic_visit_stmt(node);
        if was_block {
            self.level -= 1;
        }
    }
}

/// Compute aggregate depth profile from a collection of per-function profiles.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn aggregate_depth(profiles: &[DepthProfile]) -> DepthProfile {
    if profiles.is_empty() {
        return (0, 0.0, 0.0);
    }
    let max = profiles.iter().map(|p| p.0).max().unwrap_or(0);
    let n = profiles.len() as f32;
    let mean = profiles.iter().map(|p| p.1).sum::<f32>() / n;
    let variance = profiles.iter().map(|p| (p.1 - mean).powi(2)).sum::<f32>() / n;
    (max, mean, variance.sqrt())
}
