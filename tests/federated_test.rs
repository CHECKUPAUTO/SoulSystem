use sha2::{Digest, Sha256};
use soulsystem::federated_learning::*;

#[test]
fn test_sign_and_verify() {
    let seed = [42u8; 32];
    let keypair = KeyPair::from_seed(&seed);

    let weights = vec![0.1, 0.2, 0.3, -0.5];
    let gradient = sign_gradient(1, &weights, 1000, &keypair);

    // Verify with correct key should pass
    assert!(verify_gradient(&gradient, &keypair.public));

    // Tampered weights should fail
    let mut tampered = gradient.clone();
    tampered.weights[0] += 1.0;
    assert!(!verify_gradient(&tampered, &keypair.public));
}

#[test]
fn test_average_gradients() {
    let g1 = FederatedGradient {
        layer_id: 1,
        weights: vec![1.0, 2.0, 3.0],
        timestamp: 1,
        signature: vec![],
    };
    let g2 = FederatedGradient {
        layer_id: 1,
        weights: vec![3.0, 4.0, 5.0],
        timestamp: 2,
        signature: vec![],
    };

    let avg = average_gradients(&[g1, g2]).unwrap();
    assert_eq!(avg, vec![2.0, 3.0, 4.0]);
}

#[test]
fn test_peer_registry() {
    let mut registry = PeerRegistry::new();
    let key = [1u8; 32];
    registry.add_peer(key);
    assert!(registry.is_authorized(&key));
    assert!(!registry.is_authorized(&[2u8; 32]));
}
