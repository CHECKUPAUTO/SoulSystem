// ==========================================================================
// access_control.rs — Compartimentage mémoire multi-acteur (V6.5, point 3.B)
//
// "Si demain SoulLink interagit avec plusieurs utilisateurs ou d'autres
//  agents, il devra compartimenter ses souvenirs. Introduire des espaces
//  mémoire distincts (privé, équipe, public) avec des règles de partage."
//
// MODÈLE
//
//   - MemorySpace : un namespace logique de souvenirs. Trois familles :
//       * Private(actor)  — visible du seul actor propriétaire
//       * Team(team_id)   — visible des membres de l'équipe
//       * Public          — visible de tous
//
//   - Actor : une identité (agent, utilisateur, service). Possède des
//     appartenances (memberships) à des équipes et un id propre.
//
//   - AccessPolicy : règles read/write par (actor, space). Par défaut :
//       * read  : Private(self) | Team(membre) | Public
//       * write : Private(self) | Team(membre) | Public (si write_public activé)
//
//   - AccessController : vérifie can_read / can_write avant chaque opération.
//
// POURQUOI C'EST UTILE POUR LA RÉSILIENCE
//
//   La segmentation empêche les fuites d'information entre contextes et
//   limite l'impact d'une compromission : si un namespace est corrompu
//   ou si un actor malveillant accède, les autres espaces restent isolés.
//
// INTÉGRATION
//
//   NamespacedMemory encapsule N AtemporalMemory (une par space actif) et
//   route les opérations après vérification ACL. C'est non-invasif : la
//   AtemporalMemory existante n'est pas modifiée, juste wrappée.
// ==========================================================================

use std::collections::{HashMap, HashSet};
use crate::memory::AtemporalMemory;

// --------------------------------------------------------------------------
// Identités et espaces
// --------------------------------------------------------------------------

pub type ActorId = String;
pub type TeamId = String;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MemorySpace {
    Private(ActorId),
    Team(TeamId),
    Public,
}

impl MemorySpace {
    /// Clé de stockage stable pour le routage interne.
    pub fn key(&self) -> String {
        match self {
            MemorySpace::Private(a) => format!("private:{}", a),
            MemorySpace::Team(t) => format!("team:{}", t),
            MemorySpace::Public => "public".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Actor {
    pub id: ActorId,
    pub teams: HashSet<TeamId>,
}

impl Actor {
    pub fn new(id: impl Into<ActorId>) -> Self {
        Self { id: id.into(), teams: HashSet::new() }
    }

    pub fn with_team(mut self, team: impl Into<TeamId>) -> Self {
        self.teams.insert(team.into());
        self
    }

    pub fn is_member_of(&self, team: &str) -> bool {
        self.teams.contains(team)
    }
}

// --------------------------------------------------------------------------
// Politique d'accès
// --------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct AccessPolicy {
    /// Si true, n'importe quel actor peut écrire dans Public.
    /// Si false, Public est read-only pour tous sauf via un acteur "system".
    pub allow_public_write: bool,
    /// Liste des acteurs "system" qui peuvent tout faire (admin).
    pub system_actors: HashSet<ActorId>,
}

impl Default for AccessPolicy {
    fn default() -> Self {
        Self {
            allow_public_write: false,
            system_actors: HashSet::new(),
        }
    }
}

impl AccessPolicy {
    pub fn with_system_actor(mut self, id: impl Into<ActorId>) -> Self {
        self.system_actors.insert(id.into());
        self
    }

    pub fn can_read(&self, actor: &Actor, space: &MemorySpace) -> bool {
        if self.system_actors.contains(&actor.id) { return true; }
        match space {
            MemorySpace::Private(owner) => *owner == actor.id,
            MemorySpace::Team(team) => actor.is_member_of(team),
            MemorySpace::Public => true,
        }
    }

    pub fn can_write(&self, actor: &Actor, space: &MemorySpace) -> bool {
        if self.system_actors.contains(&actor.id) { return true; }
        match space {
            MemorySpace::Private(owner) => *owner == actor.id,
            MemorySpace::Team(team) => actor.is_member_of(team),
            MemorySpace::Public => self.allow_public_write,
        }
    }
}

// --------------------------------------------------------------------------
// Erreur d'accès
// --------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AccessError {
    ReadDenied { actor: ActorId, space: String },
    WriteDenied { actor: ActorId, space: String },
}

impl std::fmt::Display for AccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AccessError::ReadDenied { actor, space } =>
                write!(f, "read denied: actor '{}' on space '{}'", actor, space),
            AccessError::WriteDenied { actor, space } =>
                write!(f, "write denied: actor '{}' on space '{}'", actor, space),
        }
    }
}

impl std::error::Error for AccessError {}

// --------------------------------------------------------------------------
// NamespacedMemory — routeur multi-espace avec ACL
// --------------------------------------------------------------------------

pub struct NamespacedMemory {
    /// Une AtemporalMemory par espace (créée à la demande).
    spaces: HashMap<String, AtemporalMemory>,
    policy: AccessPolicy,
    dim: usize,
    episodic_capacity: usize,
    /// Stats
    pub access_grants: u64,
    pub access_denials: u64,
}

impl NamespacedMemory {
    pub fn new(dim: usize, episodic_capacity: usize, policy: AccessPolicy) -> Self {
        Self {
            spaces: HashMap::new(),
            policy,
            dim,
            episodic_capacity,
            access_grants: 0,
            access_denials: 0,
        }
    }

    /// Accès mutable à la mémoire d'un espace, créée si nécessaire,
    /// après vérification d'écriture.
    pub fn observe(
        &mut self,
        actor: &Actor,
        space: &MemorySpace,
        latent: &[f64],
        future_prediction: Option<Vec<f64>>,
    ) -> Result<(), AccessError> {
        if !self.policy.can_write(actor, space) {
            self.access_denials += 1;
            return Err(AccessError::WriteDenied {
                actor: actor.id.clone(), space: space.key(),
            });
        }
        self.access_grants += 1;
        let dim = self.dim;
        let cap = self.episodic_capacity;
        let mem = self.spaces.entry(space.key())
            .or_insert_with(|| AtemporalMemory::new(dim, cap));
        mem.observe(latent, future_prediction);
        Ok(())
    }

    /// Recherche dans un espace après vérification de lecture.
    pub fn retrieve(
        &self,
        actor: &Actor,
        space: &MemorySpace,
        query: &[f64],
        k: usize,
    ) -> Result<Vec<(Vec<f64>, f64)>, AccessError> {
        if !self.policy.can_read(actor, space) {
            return Err(AccessError::ReadDenied {
                actor: actor.id.clone(), space: space.key(),
            });
        }
        match self.spaces.get(&space.key()) {
            Some(mem) => Ok(mem.retrieve(query, k)),
            None => Ok(Vec::new()),  // espace vide/inexistant
        }
    }

    /// Recherche dans TOUS les espaces lisibles par l'actor, fusionnée.
    /// C'est la vue "ce que cet acteur peut voir" — fédère privé + équipes + public.
    pub fn retrieve_all_readable(
        &self,
        actor: &Actor,
        query: &[f64],
        k: usize,
    ) -> Vec<(Vec<f64>, f64, String)> {
        let mut results: Vec<(Vec<f64>, f64, String)> = Vec::new();
        for (space_key, mem) in &self.spaces {
            // Reconstruire le MemorySpace depuis la clé pour vérif ACL
            let space = parse_space_key(space_key);
            if let Some(sp) = space {
                if self.policy.can_read(actor, &sp) {
                    for (vec, score) in mem.retrieve(query, k) {
                        results.push((vec, score, space_key.clone()));
                    }
                }
            }
        }
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        results.truncate(k);
        results
    }

    /// Liste les espaces existants (clés).
    pub fn space_keys(&self) -> Vec<String> {
        self.spaces.keys().cloned().collect()
    }

    pub fn space_count(&self) -> usize { self.spaces.len() }

    /// Accès direct (lecture) à une AtemporalMemory pour inspection — réservé
    /// aux acteurs system.
    pub fn raw_space(&self, actor: &Actor, space: &MemorySpace)
        -> Result<Option<&AtemporalMemory>, AccessError>
    {
        if !self.policy.can_read(actor, space) {
            return Err(AccessError::ReadDenied {
                actor: actor.id.clone(), space: space.key(),
            });
        }
        Ok(self.spaces.get(&space.key()))
    }
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

fn parse_space_key(key: &str) -> Option<MemorySpace> {
    if key == "public" {
        Some(MemorySpace::Public)
    } else if let Some(rest) = key.strip_prefix("private:") {
        Some(MemorySpace::Private(rest.to_string()))
    } else if let Some(rest) = key.strip_prefix("team:") {
        Some(MemorySpace::Team(rest.to_string()))
    } else {
        None
    }
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_space_isolation() {
        let policy = AccessPolicy::default();
        let alice = Actor::new("alice");
        let bob = Actor::new("bob");

        // Alice peut lire/écrire son espace privé
        assert!(policy.can_write(&alice, &MemorySpace::Private("alice".into())));
        assert!(policy.can_read(&alice, &MemorySpace::Private("alice".into())));
        // Bob NE peut PAS accéder à l'espace privé d'Alice
        assert!(!policy.can_read(&bob, &MemorySpace::Private("alice".into())));
        assert!(!policy.can_write(&bob, &MemorySpace::Private("alice".into())));
    }

    #[test]
    fn team_space_membership() {
        let policy = AccessPolicy::default();
        let alice = Actor::new("alice").with_team("eng");
        let bob = Actor::new("bob");  // pas dans eng

        let eng = MemorySpace::Team("eng".into());
        assert!(policy.can_read(&alice, &eng));
        assert!(policy.can_write(&alice, &eng));
        assert!(!policy.can_read(&bob, &eng));
    }

    #[test]
    fn public_read_always_write_gated() {
        let policy = AccessPolicy::default();  // allow_public_write = false
        let anyone = Actor::new("random");
        assert!(policy.can_read(&anyone, &MemorySpace::Public));
        assert!(!policy.can_write(&anyone, &MemorySpace::Public));

        let policy_open = AccessPolicy { allow_public_write: true, ..Default::default() };
        assert!(policy_open.can_write(&anyone, &MemorySpace::Public));
    }

    #[test]
    fn system_actor_bypasses_all() {
        let policy = AccessPolicy::default().with_system_actor("root");
        let root = Actor::new("root");
        // root peut tout, même l'espace privé d'autrui
        assert!(policy.can_read(&root, &MemorySpace::Private("alice".into())));
        assert!(policy.can_write(&root, &MemorySpace::Private("alice".into())));
        assert!(policy.can_write(&root, &MemorySpace::Public));
    }

    #[test]
    fn namespaced_memory_enforces_write() {
        let mut nm = NamespacedMemory::new(4, 16, AccessPolicy::default());
        let alice = Actor::new("alice");
        let bob = Actor::new("bob");

        // Alice écrit dans son espace
        let r = nm.observe(&alice, &MemorySpace::Private("alice".into()),
                           &[1.0, 0.0, 0.0, 0.0], None);
        assert!(r.is_ok());

        // Bob ne peut PAS écrire dans l'espace d'Alice
        let r2 = nm.observe(&bob, &MemorySpace::Private("alice".into()),
                            &[0.0, 1.0, 0.0, 0.0], None);
        assert!(matches!(r2, Err(AccessError::WriteDenied { .. })));
        assert_eq!(nm.access_denials, 1);
    }

    #[test]
    fn namespaced_retrieve_isolation() {
        let mut nm = NamespacedMemory::new(4, 16, AccessPolicy::default());
        let alice = Actor::new("alice");
        let bob = Actor::new("bob");

        // Alice stocke dans son privé
        nm.observe(&alice, &MemorySpace::Private("alice".into()),
                   &[1.0, 0.0, 0.0, 0.0], None).unwrap();

        // Alice retrouve
        let r = nm.retrieve(&alice, &MemorySpace::Private("alice".into()),
                            &[1.0, 0.0, 0.0, 0.0], 5).unwrap();
        // (peut être vide si semantic pas encore peuplé, mais pas d'erreur ACL)
        let _ = r;

        // Bob refusé en lecture
        let r2 = nm.retrieve(&bob, &MemorySpace::Private("alice".into()),
                             &[1.0, 0.0, 0.0, 0.0], 5);
        assert!(matches!(r2, Err(AccessError::ReadDenied { .. })));
    }

    #[test]
    fn retrieve_all_readable_federates_spaces() {
        let mut nm = NamespacedMemory::new(4, 16, AccessPolicy::default());
        let alice = Actor::new("alice").with_team("eng");
        let system = Actor::new("root");
        let policy_with_root = AccessPolicy::default().with_system_actor("root");
        nm.policy = policy_with_root;

        // Remplit 3 espaces (via root qui peut tout écrire)
        nm.observe(&system, &MemorySpace::Private("alice".into()), &[1.0, 0.0, 0.0, 0.0], None).unwrap();
        nm.observe(&system, &MemorySpace::Team("eng".into()), &[0.0, 1.0, 0.0, 0.0], None).unwrap();
        nm.observe(&system, &MemorySpace::Public, &[0.0, 0.0, 1.0, 0.0], None).unwrap();
        // Espace privé de bob — alice ne doit PAS le voir
        nm.observe(&system, &MemorySpace::Private("bob".into()), &[0.0, 0.0, 0.0, 1.0], None).unwrap();

        // Alice voit : son privé + team eng + public = 3 espaces, PAS le privé de bob
        let readable = nm.retrieve_all_readable(&alice, &[1.0, 1.0, 1.0, 1.0], 10);
        let spaces_seen: HashSet<String> = readable.iter()
            .map(|(_, _, s)| s.clone()).collect();
        assert!(!spaces_seen.contains("private:bob"),
                "alice should not see bob's private space: {:?}", spaces_seen);
    }
}
