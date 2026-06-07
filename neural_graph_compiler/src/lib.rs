//! Compilateur de graphe : tri topologique (Kahn) d'un DAG de noeuds en ordre
//! d'execution. Detecte les cycles.

pub struct GraphCompiler {
    node_count: usize,
    edges: Vec<(usize, usize)>,
}

impl GraphCompiler {
    pub fn new(node_count: usize) -> Self {
        Self { node_count, edges: Vec::new() }
    }

    /// Dependance `from -> to` (from avant to). Ignore les indices hors borne.
    pub fn add_edge(&mut self, from: usize, to: usize) -> bool {
        if from < self.node_count && to < self.node_count {
            self.edges.push((from, to));
            true
        } else {
            false
        }
    }

    /// Compile en ordre d'execution (Kahn). Err si cycle.
    pub fn compile(&self) -> Result<Vec<usize>, &'static str> {
        let n = self.node_count;
        let mut indeg = vec![0usize; n];
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &(u, v) in &self.edges {
            adj[u].push(v);
            indeg[v] += 1;
        }
        let mut ready: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
        let mut order = Vec::with_capacity(n);
        while let Some(u) = ready.pop() {
            order.push(u);
            for &v in &adj[u] {
                indeg[v] -= 1;
                if indeg[v] == 0 {
                    ready.push(v);
                }
            }
        }
        if order.len() == n {
            Ok(order)
        } else {
            Err("cycle detecte : graphe non ordonnancable")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(order: &[usize], node: usize) -> usize {
        order.iter().position(|&x| x == node).unwrap()
    }

    #[test]
    fn dag_respecte_les_dependances() {
        let mut g = GraphCompiler::new(4);
        g.add_edge(0, 2);
        g.add_edge(1, 2);
        g.add_edge(2, 3);
        let order = g.compile().expect("DAG compilable");
        assert_eq!(order.len(), 4);
        assert!(pos(&order, 0) < pos(&order, 2));
        assert!(pos(&order, 1) < pos(&order, 2));
        assert!(pos(&order, 2) < pos(&order, 3));
        println!("PREUVE topo : ordre {:?} respecte les dependances", order);
    }

    #[test]
    fn cycle_detecte() {
        let mut g = GraphCompiler::new(3);
        g.add_edge(0, 1);
        g.add_edge(1, 2);
        g.add_edge(2, 0);
        assert!(g.compile().is_err());
        println!("PREUVE cycle : graphe cyclique rejete");
    }
}
