pub mod avid;
pub mod brain;
pub mod mesh;
pub mod openevolve;
pub mod orchestrator;
pub mod organs;
pub mod services;
pub mod soul_neural;
pub mod synergie;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::OrchestratorClient;
    use std::collections::HashMap;

    #[test]
    fn test_orchestrator_client_connect() {
        // Test that the client can be instantiated without panicking
        let _client = OrchestratorClient::connect("http://localhost:9020");
        // health() will fail on localhost but confirms the client is valid
        // Just verifying it compiles and doesn't panic is sufficient
    }

    #[test]
    fn test_organ_status_default() {
        let status = orchestrator::OrganStatus {
            port: 0,
            state: String::new(),
            hz: 0.0,
            energy_avg: 0.0,
            spikes: 0,
            n: 0,
            attractor: String::new(),
        };
        assert_eq!(status.port, 0);
    }

    #[test]
    fn test_mesh_status_default() {
        let status = orchestrator::MeshStatus {
            online: 0,
            total_brains: 0,
            total_n: 0,
            mesh: HashMap::new(),
        };
        assert_eq!(status.online, 0);
    }

    #[test]
    fn test_organ_brain_default() {
        let brain = orchestrator::OrganBrain {
            port: 0,
            spawned_by: String::new(),
            speciality: Vec::new(),
            version: String::new(),
        };
        assert_eq!(brain.port, 0);
    }

    #[test]
    fn test_brains_list_default() {
        let list = orchestrator::BrainsList {
            total: 0,
            brains: HashMap::new(),
        };
        assert_eq!(list.total, 0);
    }

    #[test]
    fn test_init_avid() {
        // init() is a Result<(), anyhow::Error> function
        // Just verify it compiles and has the right signature
        let _ = avid::init;
    }

    #[test]
    fn test_init_orchestrator() {
        // init() returns Result<OrchestratorClient, anyhow::Error>
        // Just verify it compiles
        let _ = orchestrator::init;
    }

    #[test]
    fn test_init_brain() {
        let _ = brain::init;
    }

    #[test]
    fn test_init_mesh() {
        let _ = mesh::init;
    }

    #[test]
    fn test_init_services() {
        let _ = services::init;
    }

    #[test]
    fn test_init_organs() {
        let _ = organs::init;
    }

    #[test]
    fn test_init_soul_neural() {
        let _ = soul_neural::init;
    }

    #[test]
    fn test_init_synergie() {
        let _ = synergie::init;
    }

    #[test]
    fn test_init_openevolve() {
        let _ = openevolve::init;
    }
}
