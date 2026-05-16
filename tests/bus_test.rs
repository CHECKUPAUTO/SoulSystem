use soulsystem::bus::{Bus, Message};

#[test]
fn test_bus_publish_subscribe() {
    let bus = Bus::new(16);

    let mut rx1 = bus.subscribe();
    let mut rx2 = bus.subscribe();

    bus.publish(Message::HnnStatus {
        ticks_per_sec: 254_000,
    });

    // rx1 should receive the message
    let msg = rx1.try_recv().expect("rx1 should receive message");
    match msg {
        Message::HnnStatus { ticks_per_sec } => assert_eq!(ticks_per_sec, 254_000),
        _ => panic!("unexpected message type"),
    }

    // rx2 should also receive the message
    let msg2 = rx2.try_recv().expect("rx2 should receive message");
    match msg2 {
        Message::HnnStatus { ticks_per_sec } => assert_eq!(ticks_per_sec, 254_000),
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
        source: "arXiv".into(),
        summary: "New paper".into(),
    });
    bus.publish(Message::EvolveOptimization {
        crate_name: "tokenjuice".into(),
        score: 0.95,
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
        Message::AvidDiscovery { source, summary } => {
            assert_eq!(source, "arXiv");
            assert_eq!(summary, "New paper");
        }
        _ => panic!("expected AvidDiscovery"),
    }

    // Check EvolveOptimization
    match rx.try_recv().unwrap() {
        Message::EvolveOptimization { crate_name, score } => {
            assert_eq!(crate_name, "tokenjuice");
            assert!((score - 0.95).abs() < f64::EPSILON);
        }
        _ => panic!("expected EvolveOptimization"),
    }
}
