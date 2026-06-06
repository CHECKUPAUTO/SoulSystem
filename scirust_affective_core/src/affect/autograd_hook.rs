use scirust::autodiff::reverse::Tape; use crate::affect::space::AffectiveState; use crate::affect::drives::DriveRegistry;
pub struct EmotionalAutogradHook { pub sensitivity: f32 }
impl EmotionalAutogradHook {
    pub fn new(sensitivity: f32) -> Self { Self { sensitivity } }
    pub fn backpropagate_emotional_tension(&self, _tape: &mut Tape, registry: &DriveRegistry, state: &AffectiveState) -> Vec<f32> {
        let _loss_val = registry.compute_homeostatic_loss(state);
        // Since the current DriveRegistry implementation is not based on AD Tensors,
        // we return a dummy gradient vector for now to allow compilation.
        vec![0.0; registry.drives.len()]
    }
    pub fn compute_weight_gate(&self, gradients: &[f32]) -> f32 {
        let magnitude = (gradients.iter().map(|g| g * g).sum::<f32>()).sqrt(); 1.0 / (1.0 + (magnitude * self.sensitivity).exp())
    }
}
