pub struct BioFeedbackActuator {
    pub sensitivity: f32,
}
impl BioFeedbackActuator {
    pub fn new(s: f32) -> Self {
        Self { sensitivity: s }
    }
}
