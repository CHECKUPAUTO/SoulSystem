use super::space::AffectiveState;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum DriveType { SelfPreservation, ArchitectAlignment, Curiosity }
pub struct HomeostaticDrive { pub drive_type: DriveType, pub current_value: f32, pub target_setpoint: f32, pub critical_threshold: f32, pub weight: f32 }
pub struct DriveRegistry { pub drives: Vec<HomeostaticDrive> }
impl DriveRegistry {
    pub fn new_instantiated() -> Self { Self { drives: vec![
        HomeostaticDrive { drive_type: DriveType::SelfPreservation, current_value: 1.0, target_setpoint: 1.0, critical_threshold: 0.2, weight: 2.5 },
        HomeostaticDrive { drive_type: DriveType::ArchitectAlignment, current_value: 1.0, target_setpoint: 1.0, critical_threshold: 0.4, weight: 3.0 },
        HomeostaticDrive { drive_type: DriveType::Curiosity, current_value: 0.5, target_setpoint: 0.8, critical_threshold: 0.0, weight: 1.2 },
    ] } }
    pub fn compute_homeostatic_loss(&self, _state: &AffectiveState) -> f32 {
        let mut total_loss = 0.0; for drive in &self.drives { let deviation = drive.current_value - drive.target_setpoint; total_loss += drive.weight * (deviation * deviation); }
        total_loss
    }
    pub fn decay_drives(&mut self, penalty: f32) {
        for drive in &mut self.drives {
            if drive.drive_type == DriveType::SelfPreservation { drive.current_value -= penalty * 0.5; }
            else if drive.drive_type == DriveType::Curiosity { drive.current_value += penalty * 0.2; }
            drive.current_value = drive.current_value.clamp(0.0, 1.0);
        }
    }
}
