use soulsystem::bus::{Bus, Message};

#[test]
fn test_bus_publish_subscribe() {
    let bus = Bus::new(16);

    let mut rx1 = bus.subscribe();
    let mut rx2 = bus.subscribe();

    bus.publish(Message::HnnStatus {
        organ: "test".to_string(),
        status: "ok".to_string(),
        energy: 100.0,
    });

    // rx1 should receive the message
    let msg = rx1.try_recv().expect("rx1 should receive message");
    match msg {
        Message::HnnStatus {
            organ: _,
            status,
            energy,
        } => {
            assert_eq!(status, "ok");
            assert!((energy - 100.0).abs() < 0.001);
        }
        _ => panic!("unexpected message type"),
    }

    // rx2 should also receive the message
    let msg2 = rx2.try_recv().expect("rx2 should receive message");
    match msg2 {
        Message::HnnStatus {
            organ: _,
            status,
            energy,
        } => {
            assert_eq!(status, "ok");
            assert!((energy - 100.0).abs() < 0.001);
        }
        _ => panic!("unexpected message type"),
    }
}

#[test]
fn test_bus_multiple_message_types() {
    let bus = Bus::new(16);
    let mut rx = bus.subscribe();

    bus.publish(Message::SynergyDetection {
        module: "AVID".into(),
        description: "Pattern found".into(),
    });
    bus.publish(Message::AvidDiscovery {
        topic: "arXiv".into(),
        summary: "New paper".into(),
    });
    bus.publish(Message::EvolveOptimization {
        generation: 42,
        best_fitness: 0.95,
    });

    // Check SynergyDetection
    match rx.try_recv().unwrap() {
        Message::SynergyDetection {
            module,
            description,
        } => {
            assert_eq!(module, "AVID");
            assert_eq!(description, "Pattern found");
        }
        _ => panic!("expected SynergyDetection"),
    }

    // Check AvidDiscovery
    match rx.try_recv().unwrap() {
        Message::AvidDiscovery { topic, summary } => {
            assert_eq!(topic, "arXiv");
            assert_eq!(summary, "New paper");
        }
        _ => panic!("expected AvidDiscovery"),
    }

    // Check EvolveOptimization
    match rx.try_recv().unwrap() {
        Message::EvolveOptimization {
            generation,
            best_fitness,
        } => {
            assert_eq!(generation, 42);
            assert!((best_fitness - 0.95).abs() < f32::EPSILON);
        }
        _ => panic!("expected EvolveOptimization"),
    }
}
