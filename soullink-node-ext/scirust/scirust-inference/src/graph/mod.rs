//! Computation graph optimizer for inference.

use crate::layers::ActivationType;
use crate::tensor::Tensor;

/// A node in the inference computation graph.
#[derive(Debug, Clone)]
pub enum GraphNode {
    Input { name: String, shape: Vec<usize> },
    Linear {
        name: String, weight: Tensor<f32>, bias: Option<Tensor<f32>>,
        activation: ActivationType,
    },
    Conv2d {
        name: String, weight: Tensor<f32>, bias: Option<Tensor<f32>>,
        kernel_size: (usize, usize), stride: (usize, usize), padding: (usize, usize),
        activation: ActivationType,
    },
    BatchNorm {
        name: String, gamma: Tensor<f32>, beta: Tensor<f32>,
        running_mean: Tensor<f32>, running_var: Tensor<f32>,
        epsilon: f32, activation: ActivationType,
    },
    Activation { name: String, kind: ActivationType },
    Constant { name: String, value: Tensor<f32> },
    Reshape { name: String, target_shape: Vec<usize> },
    Softmax { name: String },
    Concat { name: String, axis: usize },
    Output { name: String },
}

/// The optimized inference graph.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct InferenceGraph {
    nodes: Vec<GraphNode>,
    input_name: String,
    output_name: String,
}

impl InferenceGraph {
    pub fn new(input_name: &str, output_name: &str) -> Self {
        Self { nodes: Vec::new(), input_name: input_name.to_string(), output_name: output_name.to_string() }
    }

    pub fn add_node(&mut self, node: GraphNode) { self.nodes.push(node); }
    pub fn nodes(&self) -> &[GraphNode] { &self.nodes }

    pub fn optimize(&mut self) {
        self.fuse_activations();
    }

    fn fuse_activations(&mut self) {
        let mut i = 0;
        while i + 1 < self.nodes.len() {
            let activation = match &self.nodes[i + 1] {
                GraphNode::Activation { kind, .. } => *kind,
                _ => { i += 1; continue; }
            };

            let fused = match &mut self.nodes[i] {
                GraphNode::Linear { activation: ref mut a, .. } => { *a = activation; true }
                GraphNode::Conv2d { activation: ref mut a, .. } => { *a = activation; true }
                GraphNode::BatchNorm { activation: ref mut a, .. } => { *a = activation; true }
                _ => false,
            };

            if fused { self.nodes.remove(i + 1); }
            i += 1;
        }
    }

    pub fn estimate_flops(&self) -> u64 {
        let mut total = 0u64;
        for node in &self.nodes {
            match node {
                GraphNode::Linear { weight, .. } => {
                    let m = weight.shape()[0]; let k = weight.shape()[1];
                    total += (2 * k) as u64 * m as u64;
                }
                GraphNode::Conv2d { weight, kernel_size, .. } => {
                    let oc = weight.shape()[0]; let ic = weight.shape()[1];
                    total += (2 * ic * kernel_size.0 * kernel_size.1) as u64 * oc as u64;
                }
                _ => {}
            }
        }
        total
    }

    pub fn num_parameters(&self) -> usize {
        let mut count = 0;
        for node in &self.nodes {
            match node {
                GraphNode::Linear { weight, bias, .. } => {
                    count += weight.len() + bias.as_ref().map_or(0, |b| b.len());
                }
                GraphNode::Conv2d { weight, bias, .. } => {
                    count += weight.len() + bias.as_ref().map_or(0, |b| b.len());
                }
                GraphNode::BatchNorm { gamma, beta, .. } => count += gamma.len() + beta.len(),
                _ => {}
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_fusion() {
        let mut graph = InferenceGraph::new("input", "output");
        graph.add_node(GraphNode::Input { name: "input".into(), shape: vec![1, 4] });
        graph.add_node(GraphNode::Linear {
            name: "fc1".into(), weight: Tensor::<f32>::rand(&[3, 4]),
            bias: Some(Tensor::<f32>::zeros(&[3])), activation: ActivationType::None,
        });
        graph.add_node(GraphNode::Activation { name: "relu1".into(), kind: ActivationType::ReLU });
        graph.add_node(GraphNode::Output { name: "output".into() });

        let before = graph.nodes().len();
        graph.optimize();
        let after = graph.nodes().len();
        assert!(after < before, "Activation should fuse into linear layer");
    }

    #[test]
    fn test_flops_estimation() {
        let mut graph = InferenceGraph::new("input", "output");
        graph.add_node(GraphNode::Linear {
            name: "fc1".into(), weight: Tensor::<f32>::rand(&[256, 784]),
            bias: None, activation: ActivationType::ReLU,
        });
        assert!(graph.estimate_flops() > 0);
    }
}
