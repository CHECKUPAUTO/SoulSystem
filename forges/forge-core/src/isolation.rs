//! Gestion de l'isolation d'exécution pour les candidats de type "code".
//! Permet de lancer la compilation ou les benchmarks dans un sous-processus
//! supervisé par un garde-fou (timeout) afin de protéger le moteur principal
//! contre les boucles infinies, paniques ou corruptions de mémoire.

use crate::error::{ForgeError, Result};
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Exécute une commande système (ex: `cargo bench`) avec un timeout strict.
/// Retourne la sortie standard (stdout) en cas de succès, coupe le processus
/// et renvoie une variante d'erreur explicite en cas de dépassement ou de crash.
pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Result<String> {
    // On redirige stdout et stderr pour capturer finement les diagnostics
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            ForgeError::Evaluation(format!("Impossible de spawn le processus candidat: {e}"))
        })?;

    let start = Instant::now();

    loop {
        match child.try_wait() {
            // Le processus s'est terminé proprement
            Ok(Some(status)) => {
                if status.success() {
                    let mut stdout_str = String::new();
                    if let Some(mut stdout) = child.stdout.take() {
                        let _ = stdout.read_to_string(&mut stdout_str);
                    }
                    return Ok(stdout_str);
                } else {
                    let mut stderr_str = String::new();
                    if let Some(mut stderr) = child.stderr.take() {
                        let _ = stderr.read_to_string(&mut stderr_str);
                    }
                    return Err(ForgeError::Evaluation(format!(
                        "Échec d'exécution du code généré (code de sortie: {status}). Stderr: {stderr_str}"
                    )));
                }
            }
            // Le processus est toujours en cours d'exécution
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill(); // Destruction immédiate du processus récalcitrant
                    let _ = child.wait(); // Nettoyage pour éviter les processus zombies
                    return Err(ForgeError::Evaluation(format!(
                        "Timeout dépassé ({:?}) : boucle infinie ou blocage détecté. Candidat éliminé.",
                        timeout
                    )));
                }
                // Pause courte pour éviter de saturer le cœur CPU de supervision
                thread::sleep(Duration::from_millis(15));
            }
            // Erreur système de bas niveau durant le polling
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ForgeError::Evaluation(format!(
                    "Erreur système d'interrogation de processus: {e}"
                )));
            }
        }
    }
}
