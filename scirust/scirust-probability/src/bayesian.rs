//! Bayesian network inference engine.

use crate::models::{Prior, Posterior, Rng, XorShiftRng};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// BayesianNode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BayesianNode {
    pub id: String,
    pub prior: Prior,
    pub observations: Vec<f64>,
    pub discrete: bool,
    pub categories: Vec<String>,
    pub cpt: Option<HashMap<Vec<usize>, Vec<f64>>>,
}

impl BayesianNode {
    pub fn new_continuous(id: &str, prior: Prior) -> Self {
        Self {
            id: id.to_string(),
            prior,
            observations: Vec::new(),
            discrete: false,
            categories: Vec::new(),
            cpt: None,
        }
    }

    pub fn new_discrete(id: &str, categories: Vec<String>, prior: Prior) -> Self {
        Self {
            id: id.to_string(),
            prior,
            observations: Vec::new(),
            discrete: true,
            categories,
            cpt: None,
        }
    }

    pub fn set_cpt(&mut self, cpt: HashMap<Vec<usize>, Vec<f64>>) {
        self.cpt = Some(cpt);
    }

    pub fn observe(&mut self, value: f64) {
        self.observations.push(value);
    }

    pub fn posterior(&self) -> Posterior {
        Posterior::with_data(self.prior.clone(), self.observations.clone())
    }

    pub fn prior_dist(&self) -> &Prior {
        &self.prior
    }
}

// ---------------------------------------------------------------------------
// BayesianNetwork
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BayesianNetwork {
    pub nodes: HashMap<String, BayesianNode>,
    pub parents: HashMap<String, Vec<String>>,
    pub children: HashMap<String, Vec<String>>,
    pub weights: HashMap<(String, String), f64>,
    pub topo_order: Vec<String>,
}

impl BayesianNetwork {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            parents: HashMap::new(),
            children: HashMap::new(),
            weights: HashMap::new(),
            topo_order: Vec::new(),
        }
    }

    pub fn add_node(&mut self, node: BayesianNode) {
        let id = node.id.clone();
        self.nodes.insert(id.clone(), node);
        self.parents.entry(id.clone()).or_default();
        self.children.entry(id.clone()).or_default();
        self.update_topo();
    }

    pub fn add_edge(&mut self, parent: &str, child: &str, weight: f64) {
        self.parents.entry(child.to_string())
            .or_default()
            .push(parent.to_string());
        self.children.entry(parent.to_string())
            .or_default()
            .push(child.to_string());
        self.weights.insert((parent.to_string(), child.to_string()), weight);
        self.update_topo();
    }

    pub fn get_parents(&self, id: &str) -> Vec<&BayesianNode> {
        self.parents.get(id)
            .map(|p| p.iter().filter_map(|pid| self.nodes.get(pid)).collect())
            .unwrap_or_default()
    }

    pub fn get_children(&self, id: &str) -> Vec<&BayesianNode> {
        self.children.get(id)
            .map(|c| c.iter().filter_map(|cid| self.nodes.get(cid)).collect())
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize { self.nodes.len() }
    pub fn is_empty(&self) -> bool { self.nodes.is_empty() }

    fn update_topo(&mut self) {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for id in self.nodes.keys() {
            in_degree.insert(id.clone(), 0);
        }
        for (child, pars) in &self.parents {
            for _p in pars {
                *in_degree.entry(child.clone()).or_insert(0) += 1;
            }
        }

        let mut queue: Vec<String> = in_degree.iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut order = Vec::new();
        while !queue.is_empty() {
            queue.sort();
            let id = queue.remove(0);
            order.push(id.clone());

            if let Some(children) = self.children.get(&id) {
                for child in children {
                    if let Some(deg) = in_degree.get_mut(child) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(child.clone());
                        }
                    }
                }
            }
        }
        self.topo_order = order;
    }

    // -----------------------------------------------------------------------
    // Exact inference
    // -----------------------------------------------------------------------

    pub fn enumeration_ask(
        &self,
        query_var: &str,
        evidence: &HashMap<String, f64>,
    ) -> f64 {
        if let Some(node) = self.nodes.get(query_var) {
            if node.discrete {
                let parent_ids = self.parents.get(query_var)
                    .cloned().unwrap_or_default();
                let parent_vals: Vec<usize> = parent_ids.iter()
                    .filter_map(|pid| evidence.get(pid))
                    .map(|&v| v as usize)
                    .collect();

                if let Some(ref cpt) = node.cpt {
                    if let Some(probs) = cpt.get(&parent_vals) {
                        return probs[0];
                    }
                }
            }
            let posterior = node.posterior();
            return posterior.mean();
        }
        f64::NAN
    }

    // -----------------------------------------------------------------------
    // Gibbs sampling
    // -----------------------------------------------------------------------

    pub fn gibbs_sample(
        &self,
        query_vars: &[String],
        evidence: &HashMap<String, f64>,
        num_samples: usize,
        burn_in: usize,
    ) -> HashMap<String, f64> {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64;
        let mut rng = XorShiftRng::new(seed);
        let node_ids: Vec<String> = self.nodes.keys().cloned().collect();

        let mut state: HashMap<String, f64> = evidence.clone();
        for id in &node_ids {
            if !state.contains_key(id) {
                if let Some(node) = self.nodes.get(id) {
                    state.insert(id.clone(), node.prior.sample(&mut rng));
                }
            }
        }

        let mut samples: Vec<HashMap<String, f64>> = Vec::new();

        for step in 0..(num_samples + burn_in) {
            let mut order: Vec<&String> = node_ids.iter()
                .filter(|id| !evidence.contains_key(*id))
                .collect();
            // Manual Fisher-Yates shuffle
            for i in (1..order.len()).rev() {
                let j = (rng.gen_range(0.0..1.0) * (i + 1) as f64) as usize;
                order.swap(i, j);
            }

            for id in &order {
                if let Some(node) = self.nodes.get(*id) {
                    let sample = node.prior.sample(&mut rng);
                    state.insert((*id).clone(), sample);
                }
            }

            if step >= burn_in {
                samples.push(state.clone());
            }
        }

        let mut results = HashMap::new();
        for qv in query_vars {
            let values: Vec<f64> = samples.iter()
                .filter_map(|s| s.get(qv))
                .copied()
                .collect();

            if !values.is_empty() {
                let mean = values.iter().sum::<f64>() / values.len() as f64;
                results.insert(qv.clone(), mean);
            }
        }
        results
    }

    // -----------------------------------------------------------------------
    // Belief Propagation
    // -----------------------------------------------------------------------

    pub fn belief_propagation(
        &self,
        evidence: &HashMap<String, f64>,
        max_iters: usize,
    ) -> HashMap<String, f64> {
        let mut marginals: HashMap<String, f64> = evidence.clone();

        for _iter in 0..max_iters {
            for (id, node) in &self.nodes {
                if evidence.contains_key(id) { continue; }
                let posterior = node.posterior();
                marginals.insert(id.clone(), posterior.mean());
            }

            for (id, node) in &self.nodes {
                let children = self.children.get(id)
                    .cloned().unwrap_or_default();

                for child in &children {
                    if let Some(_child_node) = self.nodes.get(child) {
                        let weight = self.weights.get(&(id.clone(), child.clone()))
                            .copied().unwrap_or(1.0);
                        let parent_val = marginals.get(id).copied().unwrap_or(0.0);
                        let child_val = marginals.get(child).copied().unwrap_or(0.0);
                        let msg_val = parent_val * weight + child_val * (1.0 - weight);
                        marginals.insert(child.clone(), msg_val);
                    }
                }
            }
        }
        marginals
    }

    // -----------------------------------------------------------------------
    // Explanation & Sampling
    // -----------------------------------------------------------------------

    pub fn explain(&self, node_id: &str, evidence: &HashMap<String, f64>) -> String {
        let mut explanation = String::new();
        if let Some(node) = self.nodes.get(node_id) {
            explanation.push_str(&format!("Inference for node '{}':\n", node_id));
            explanation.push_str(&format!("  Prior: {}\n", node.prior));

            let parent_ids = self.parents.get(node_id).cloned().unwrap_or_default();
            if !parent_ids.is_empty() {
                explanation.push_str("  Parents:\n");
                for pid in &parent_ids {
                    let weight = self.weights.get(&(pid.clone(), node_id.to_string()))
                        .copied().unwrap_or(0.0);
                    let val = evidence.get(pid).map(|v| format!("{}", v))
                        .unwrap_or_else(|| "unobserved".to_string());
                    if let Some(pnode) = self.nodes.get(pid) {
                        explanation.push_str(&format!(
                            "    - {}: prior={}, evidence={}, weight={:.2}\n",
                            pid, pnode.prior, val, weight
                        ));
                    }
                }
            }

            let child_ids = self.children.get(node_id).cloned().unwrap_or_default();
            if !child_ids.is_empty() {
                explanation.push_str("  Children:\n");
                for cid in &child_ids {
                    let val = evidence.get(cid).map(|v| format!("{}", v))
                        .unwrap_or_else(|| "unobserved".to_string());
                    explanation.push_str(&format!("    - {}: evidence={}\n", cid, val));
                }
            }

            if !node.observations.is_empty() {
                explanation.push_str(&format!("  Observations: {:?}\n", node.observations));
                let post = node.posterior();
                explanation.push_str(&format!("  Posterior MAP: {:.4}\n", post.map_estimate()));
            }
        } else {
            explanation.push_str(&format!("Node '{}' not found in network.\n", node_id));
        }
        explanation
    }

    pub fn sample(&self, n: usize) -> Vec<HashMap<String, f64>> {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64;
        let mut rng = XorShiftRng::new(seed);
        let mut samples = Vec::with_capacity(n);

        for _ in 0..n {
            let mut sample = HashMap::new();
            for id in &self.topo_order {
                if let Some(node) = self.nodes.get(id) {
                    sample.insert(id.clone(), node.prior.sample(&mut rng));
                }
            }
            samples.push(sample);
        }
        samples
    }
}

// ---------------------------------------------------------------------------
// BayesianInferenceEngine
// ---------------------------------------------------------------------------

pub struct BayesianInferenceEngine {
    network: BayesianNetwork,
}

impl BayesianInferenceEngine {
    pub fn new(network: BayesianNetwork) -> Self {
        Self { network }
    }

    pub fn update(&mut self, node_id: &str, observation: f64) -> Result<(), String> {
        if let Some(node) = self.network.nodes.get_mut(node_id) {
            node.observe(observation);
            Ok(())
        } else {
            Err(format!("Node '{}' not found", node_id))
        }
    }

    pub fn query(&self, node_id: &str, evidence: &HashMap<String, f64>) -> Result<f64, String> {
        Ok(self.network.enumeration_ask(node_id, evidence))
    }

    pub fn explain(&self, node_id: &str, evidence: &HashMap<String, f64>) -> String {
        self.network.explain(node_id, evidence)
    }

    pub fn gibbs(
        &self,
        query_vars: &[String],
        evidence: &HashMap<String, f64>,
        samples: usize,
    ) -> HashMap<String, f64> {
        self.network.gibbs_sample(query_vars, evidence, samples, samples / 10)
    }

    pub fn sample(&self, n: usize) -> Vec<HashMap<String, f64>> {
        self.network.sample(n)
    }

    pub fn network(&self) -> &BayesianNetwork { &self.network }
    pub fn network_mut(&mut self) -> &mut BayesianNetwork { &mut self.network }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Prior;

    #[test]
    fn test_bayesian_network_creation() {
        let mut net = BayesianNetwork::new();
        net.add_node(BayesianNode::new_continuous("rain", Prior::Beta(2.0, 5.0)));
        net.add_node(BayesianNode::new_continuous("sprinkler", Prior::Beta(3.0, 3.0)));
        assert_eq!(net.len(), 2);
    }

    #[test]
    fn test_bayesian_network_edges() {
        let mut net = BayesianNetwork::new();
        net.add_node(BayesianNode::new_continuous("rain", Prior::Beta(2.0, 5.0)));
        net.add_node(BayesianNode::new_continuous("wet_grass", Prior::Beta(1.0, 1.0)));
        net.add_edge("rain", "wet_grass", 0.9);

        let parents = net.get_parents("wet_grass");
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].id, "rain");
    }

    #[test]
    fn test_bayesian_inference_engine() {
        let mut net = BayesianNetwork::new();
        net.add_node(BayesianNode::new_continuous("x", Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 }));

        let mut engine = BayesianInferenceEngine::new(net);
        engine.update("x", 2.0).unwrap();
        engine.update("x", 2.5).unwrap();

        let evidence = HashMap::new();
        let result = engine.query("x", &evidence).unwrap();
        assert!(result.is_finite());
    }

    #[test]
    fn test_gibbs_sampling() {
        let mut net = BayesianNetwork::new();
        net.add_node(BayesianNode::new_continuous("a", Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 }));
        net.add_node(BayesianNode::new_continuous("b", Prior::Gaussian { mu: 5.0, sigma_sq: 1.0 }));
        net.add_edge("a", "b", 0.5);

        let results = net.gibbs_sample(
            &["a".to_string(), "b".to_string()],
            &HashMap::new(), 500, 50
        );
        assert!(results.contains_key("a"));
        assert!(results.contains_key("b"));
    }

    #[test]
    fn test_explain() {
        let mut net = BayesianNetwork::new();
        net.add_node(BayesianNode::new_continuous("x", Prior::Beta(2.0, 2.0)));
        let explanation = net.explain("x", &HashMap::new());
        assert!(explanation.contains("Inference"));
        assert!(explanation.contains("Beta"));
    }

    #[test]
    fn test_joint_sampling() {
        let mut net = BayesianNetwork::new();
        net.add_node(BayesianNode::new_continuous("a", Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 }));
        net.add_node(BayesianNode::new_continuous("b", Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 }));
        net.add_edge("a", "b", 0.8);

        let samples = net.sample(100);
        assert_eq!(samples.len(), 100);
        for s in &samples {
            assert!(s.contains_key("a"));
            assert!(s.contains_key("b"));
        }
    }

    #[test]
    fn test_belief_propagation() {
        let mut net = BayesianNetwork::new();
        net.add_node(BayesianNode::new_continuous("a", Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 }));
        net.add_node(BayesianNode::new_continuous("b", Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 }));
        net.add_edge("a", "b", 0.7);

        let marginals = net.belief_propagation(&HashMap::new(), 10);
        assert!(marginals.contains_key("a"));
        assert!(marginals.contains_key("b"));
    }

    #[test]
    fn test_dag_topo_order() {
        let mut net = BayesianNetwork::new();
        net.add_node(BayesianNode::new_continuous("a", Prior::Uniform { a: 0.0, b: 1.0 }));
        net.add_node(BayesianNode::new_continuous("b", Prior::Uniform { a: 0.0, b: 1.0 }));
        net.add_node(BayesianNode::new_continuous("c", Prior::Uniform { a: 0.0, b: 1.0 }));
        net.add_edge("a", "b", 1.0);
        net.add_edge("b", "c", 1.0);

        let order = &net.topo_order;
        let pos_a = order.iter().position(|x| x == "a").unwrap();
        let pos_b = order.iter().position(|x| x == "b").unwrap();
        let pos_c = order.iter().position(|x| x == "c").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }
}
