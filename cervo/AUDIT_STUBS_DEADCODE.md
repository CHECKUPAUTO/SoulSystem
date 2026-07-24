# Audit ciblé — stubs, placeholders, absences de liaison, code mort (CERVO v2)

Audit demandé : recherche minutieuse de **stubs, TODOs, placeholders, absences
de liaison (code écrit mais jamais branché), code mort, valeurs codées en dur
faisant semblant de calculer, simulations déguisées en implémentations réelles**.

Périmètre : lecture intégrale des 15 fichiers source. `cargo check --all-targets`
et `cargo clippy` → **0 warning** (les items `pub` inutilisés n'étant pas
signalés par le lint `dead_code`, chaque item ci-dessous a été confirmé par
`grep` : zéro appelant hors définition/tests). Aucun marqueur littéral
`TODO`/`FIXME`/`unimplemented!`/`placeholder`/`mock`/`dummy` dans le code.

## État — rendu opérationnel dans cette branche

> Les findings ci-dessous ont été **corrigés** : le cerveau autonome exécute
> désormais une boucle réelle de bout en bout (vérifiée par `cargo run` +
> 66 tests). Résumé des câblages ajoutés :
>
> - **Traitement de flux** : `process_data` est câblé (unités + `Cortex::process_all`) ;
>   les unités appliquent réellement leur transformation à un flux de données,
>   recall/mémoire relue, sorties mémorisées.
> - **Échange swarm** : les sorties sont **diffusées** (`SwarmMessage::Data`) et
>   ingérées par les pairs ; provenance (`source_unit`, `created_at`) tamponnée.
> - **Protocole de santé vivant** : `HealthReport`/`Heartbeat` émis sur cycle et
>   consommés (`peer_health`) — `pairs_connus` observable.
> - **Adoption inter-unités** : `SyncRequest`/`AlgorithmShare` + `config_json` +
>   `TransformationRegistry` → transfert horizontal du meilleur algorithme (vérifié).
> - **Stabilité réelle** : `run_sandbox_test` consulte `Transformation::is_stable()` ;
>   `is_stable` n'est plus codé en dur et **conditionne l'adoption** d'une mutation.
> - **Config** : `validate` (via `Cortex::try_new`) + `save`/`load` (roundtrip démontré).
> - **Arrêt coordonné** : `broadcast_shutdown` via le bus.
> - **Code mort supprimé** : `struct Unit`, `SafeState`/rollback décoratif,
>   variante `MutationRequest` ; champs `RejectedMutation` désormais lus
>   (`last_rejection`), provenance renseignée.
>
> Le texte d'origine est conservé ci-dessous comme état initial de l'audit.

## Constat global (état initial de l'audit)

Le pattern dominant n'est pas le stub explicite mais la **sur-spécification
débranchée** : de nombreuses capacités (échange de données inter-unités,
protocole santé/heartbeat, persistance, validation de config, pipeline
déclaratif, recall mémoire) sont écrites et unit-testées mais **jamais reliées
au flux d'exécution**. S'y ajoutent quelques valeurs qui **prétendent être
calculées** (`is_stable`, rollback `restore`, `created_at`).

Flux principal réellement exercé (`main.rs` → 5 démos) : `Memory`
(store/consolidate/decay/snapshot), `PipelineTransform::new`+`transform`,
`UnitHandle::{with_swarm,with_config,mutate,get_state}`,
`Cortex::{spawn,tick_all,get,report}`, `dynamics::cycle_step`,
`stability::run_sandbox_test`, `EvolutionTracker`.

---

## Critique / élevé — « fait semblant » ou capacité centrale débranchée

- **`swarm.rs:22-48` — protocole à 8 variants, 6 morts.** `SwarmMessage` définit
  `SyncRequest`, `AlgorithmShare`, `HealthReport`, `MutationRequest`, `Shutdown`,
  `Heartbeat` — aucun n'est jamais **publié ni traité** hors tests. Seul
  `MutationAnnounce` circule (publié `units.rs:353`, traité `units.rs:291`).
  Le volet santé/heartbeat/partage-d'algo du « cerveau autonome » est décoratif.
- **`units.rs:172-179` — `process_data` jamais appelé.** C'est le seul chemin
  qui exécute réellement un algorithme **sur des données**. Ni `main.rs` ni
  `cortex.rs` ne font traiter de données aux unités : la démo ne fait que
  **muter**. L'« écosystème auto-évolutif » n'exécute jamais ses transformations
  sur un flux de données réel.
- **`units.rs:302` — handler `SwarmMessage::Data` sans émetteur.** Le code sait
  stocker un payload reçu, mais `SwarmMessage::Data` n'est **publié nulle part**
  (`rg` → 1 seule ligne, le handler). Les unités n'échangent jamais de données.
- **`stability.rs:53` — `is_stable: true` codé en dur.** `run_sandbox_test`
  renvoie toujours « stable » dès qu'on atteint `Ok`. Le trait
  `Transformation::is_stable()` n'est **jamais consulté** dans le chemin
  d'acceptation ; le rejet ne se fait que par timeout/taille/divergence.
  Le champ prétend être un verdict calculé alors qu'il est constant.
- **`core/data.rs:10-16` — provenance factice.** `DataMetadata.created_at`,
  `source_unit`, `mime_type` ne sont **assignés nulle part** ; seul `tags` est
  peuplé. `created_at` vaut donc toujours `0` — un faux horodatage.

## Moyen

- **`units.rs:77-88, 311, 342` — rollback décoratif.** `save_state` capture
  `rsi_index`+`saturation`, `restore` les réécrit sur rejet de mutation. Or rien
  dans `handle_mutation` ne modifie ces champs (ils ne changent que dans
  `Cycle`, `units.rs:236-241`) → no-op qui met en scène une garantie de sûreté
  inexistante.
- **`core/data.rs:60-77` — `struct Unit` jamais construite.** Le runtime utilise
  `ActorState`. Contient `rsi_index: 50.0` codé en dur. Morte.
- **`config.rs:101, 106, 111` — `save`/`load`/`validate` jamais appelés.**
  `load()` ne valide pas ce qu'il charge ; la validation de config existe mais
  n'est branchée nulle part.
- **`memory.rs:123-174, 278-291` — `recall`/`recall_by_tag`/`save`/`load`
  jamais appelés au runtime.** Les unités stockent et décroissent mais ne
  **rappellent jamais** rien : la mémoire est écrite puis jamais relue.
- **`pipeline.rs:57, 113-176` — couche « config-driven » débranchée.**
  `from_config`, `TransformationRegistry` (+ son `Default` enregistrant 4
  transforms), `PipelineConfig`, `make_identity_reverse_pipeline`,
  `make_reverse_amplify_pipeline` ne servent qu'aux tests ; `main` construit les
  pipelines à la main via `PipelineTransform::new`.

## Mineur

- **`units.rs:34-40` — `#[allow(dead_code)]` sur `RejectedMutation`.** L'attribut
  masque le fait que `timestamp`, `attempted_algorithm`, `reason` sont écrits
  (`units.rs:337`) mais jamais lus : seul `rejected_mutations.len()` est consommé
  (`units.rs:279`). Le « pourquoi/quoi/quand » d'un rejet est collecté puis jeté.
- **`evolution.rs:118, 122, 138`** — `get_family_score`, `merge`,
  `decay_old_scores` jamais appelés hors tests (donc `EvolutionScore.last_success`
  mort par transitivité).
- **`cortex.rs:51, 60, 70, 74, 78`** — `kill`, `kill_all`, `unit_count`,
  `swarm`, `config` jamais appelés (`main` ne détruit jamais d'unité).
- **`units.rs:110, 148`** — `UnitHandle::new` et `set_rsi` jamais appelés hors
  définition.
- **`swarm.rs:77, 89, 93, 97`** — `sender`, `is_full`, `len`, `is_empty` (API du
  bus) non utilisés hors tests.
- **`core/transform.rs:16`** — méthode par défaut `describe()` jamais invoquée
  (override en `pipeline.rs:108`, mais personne ne l'appelle).
- **`stability.rs:106`** — `create_labeled_test_data` exportée mais utilisée
  seulement dans son propre test (le runtime utilise `create_test_data`).

## Info (nommage, pas un stub)

- **`dynamics.rs:3-15`** — `apply_attraction` **augmente** `rsi` et
  `apply_repulsion` le **diminue** (appelées quand `rsi < seuil_bas` /
  `rsi > seuil_haut` : attraction vers le centre, donc cohérent). Le calcul est
  réel, mais les noms et les noms de tests (`test_attraction_lowers_rsi` qui
  asserte `rsi > 10.0`) sont contradictoires.

---

## Récapitulatif — branchement au flux principal

| Module | Branché au runtime ? |
|---|---|
| `core/data.rs` | `Data`/`SafeState` oui ; **`Unit` jamais construit** ; 3 champs de `DataMetadata` jamais remplis |
| `core/error.rs`, `core/mod.rs`, `lib.rs`, `main.rs` | Propres |
| `core/transform.rs` | Oui (sauf `describe()`) |
| `config.rs` | Config oui ; **`save`/`load`/`validate` jamais appelés** |
| `dynamics.rs` | Oui (calcul réel ; nommage trompeur) |
| `evolution.rs` | Partiel : `merge`/`decay_old_scores`/`get_family_score` non branchés |
| `memory.rs` | store/decay/snapshot oui ; **recall/save/load jamais appelés au runtime** |
| `pipeline.rs` | `new` oui ; **couche config-driven non branchée** |
| `stability.rs` | Oui, mais `is_stable` codé en dur |
| `swarm.rs` | **2 variants/8 utilisés ; 6 morts** |
| `units.rs` | Oui ; mais `process_data`/`set_rsi`/`new` non branchés |
| `cortex.rs` | spawn/tick_all/get/report oui ; kill/swarm/config non branchés |

*Aucune modification de code n'a été apportée par cet audit — document
d'analyse uniquement.*
