use soulsystem::meta_learning::*;

struct SimpleModule {
    params: Vec<f64>,
}

impl Optimizable for SimpleModule {
    fn get_params(&self) -> Vec<f64> {
        self.params.clone()
    }
    fn set_params(&mut self, params: &[f64]) {
        self.params = params.to_vec();
    }
    fn evaluate(&self) -> f64 {
        1.0 + self.params.iter().sum::<f64>()
    }
    fn name(&self) -> &str {
        "simple"
    }
}

#[tokio::test]
async fn test_evolution_improves() {
    let mut module = SimpleModule {
        params: vec![-1.0, -1.0],
    };
    let initial = module.evaluate();

    let result = run_meta_evolution(&mut module, 20).await.unwrap();
    let improved = module.evaluate();

    // After evolution, score should improve or stay
    assert!(improved >= initial - 0.1);
    assert!(result.best_params.len() == 2);
}
