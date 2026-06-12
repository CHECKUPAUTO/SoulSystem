use std::collections::BTreeMap;

use rustpython_ast::text_size::TextRange;
use rustpython_ast::{Expr, Stmt, Visitor};

/// Extract (caller, callee) edges from function bodies via [`Visitor`].
pub fn extract_edges(suite: &[Stmt<TextRange>]) -> BTreeMap<(String, String), u32> {
    let mut collector = EdgeCollector {
        edges: BTreeMap::new(),
        current_func: None,
    };
    for stmt in suite {
        collector.visit_stmt(stmt.clone());
    }
    collector.edges
}

struct EdgeCollector {
    edges: BTreeMap<(String, String), u32>,
    current_func: Option<String>,
}

impl Visitor<TextRange> for EdgeCollector {
    fn visit_stmt(&mut self, node: Stmt<TextRange>) {
        match &node {
            Stmt::FunctionDef(fd) => {
                let prev = self.current_func.replace(fd.name.to_string());
                self.generic_visit_stmt(node);
                self.current_func = prev;
                return;
            }
            Stmt::AsyncFunctionDef(fd) => {
                let prev = self.current_func.replace(fd.name.to_string());
                self.generic_visit_stmt(node);
                self.current_func = prev;
                return;
            }
            _ => {}
        }
        self.generic_visit_stmt(node);
    }

    fn visit_expr(&mut self, node: Expr<TextRange>) {
        if let Expr::Call(call) = &node {
            if let Some(ref caller) = self.current_func {
                let callee = callee_name(&call.func);
                *self.edges.entry((caller.clone(), callee)).or_insert(0) += 1;
            }
        }
        self.generic_visit_expr(node);
    }
}

fn callee_name(func: &Expr<TextRange>) -> String {
    match func {
        Expr::Name(n) => n.id.to_string(),
        Expr::Attribute(a) => a.attr.to_string(),
        _ => "<unknown>".to_string(),
    }
}
