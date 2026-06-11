use avid_gomogo::cli::{Cli, Mode};
use avid_gomogo::config::GomogoConfig;
use avid_gomogo::{captcha, proxy, scanner};
use clap::Parser;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let cfg = GomogoConfig::load(cli.config.as_deref())?;

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cfg.logging.level));

    if cfg.logging.json_format {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    }
    tracing_log::LogTracer::init().ok();

    match cli.mode {
        Mode::Scanner(args) => scanner::run_scanner(args, &cfg).await,
        Mode::Proxy(args) => proxy::run_proxy(args, &cfg).await,
        Mode::Techniques => {
            print_techniques();
            Ok(())
        }
        Mode::Captcha(args) => {
            let port = args.port.unwrap_or(cfg.captcha.default_port);
            let bind = args
                .bind
                .unwrap_or_else(|| cfg.captcha.default_bind.clone());
            captcha::run_server(&bind, port, &cfg).await?;
            Ok(())
        }
        Mode::Init(init_args) => {
            GomogoConfig::generate_default(&init_args.output)?;
            info!("Configuration générée dans {}", init_args.output.display());
            Ok(())
        }
    }
}

fn print_techniques() {
    println!("=== TECHNIQUES DE REVERSE ENGINEERING D'API ===");
    println!();
    println!("1. DÉCOUVERTE DE SURFACE");
    println!("   - Bruteforce de chemins (wordlists, Google dorks, Wayback Machine)");
    println!("   - Analyse du trafic réseau (mitmproxy, Wireshark, Charles)");
    println!("   - Extraction depuis le code source (APK, IPA, bundles JS)");
    println!("   - Utilisation de fichiers robots.txt, sitemap.xml, swagger.json, openapi.yaml");
    println!();
    println!("2. ANALYSE DU TRAFIC");
    println!("   - Proxy MITM avec certificat personnalisé pour HTTPS (mode --proxy)");
    println!("   - Capture et rejeu de requêtes (Burp Suite, Postman)");
    println!("   - Comparaison des réponses pour détecter des paramètres cachés (mass assignment)");
    println!();
    println!("3. FUZZING DE PARAMÈTRES");
    println!("   - Ajout de paramètres inattendus (admin=true, debug=1)");
    println!("   - Bruteforce de noms de paramètres (wordlists de paramètres courants)");
    println!("   - Envoi de types de données erronés (null, tableaux, objets profonds)");
    println!("   - Pollution de paramètres (HPP – envoyer plusieurs fois le même paramètre)");
    println!();
    println!("4. ATTAQUES SUR L'AUTHENTIFICATION");
    println!("   - Test de tokens JWT mal configurés (none algorithm, clé faible)");
    println!("   - Réutilisation de tokens entre différents rôles (vertical privilege escalation)");
    println!("   - Contournement OAuth2 (redirect_uri, state, PKCE)");
    println!();
    println!("5. ANALYSE DES RÉPONSES");
    println!("   - Diffing de réponses (comparer admin vs user, plans tarifaires)");
    println!("   - Recherche de clés API, tokens, secrets dans les réponses");
    println!("   - Analyse des en-têtes personnalisés (X-*, rate limits, versions)");
    println!();
    println!("6. ATTAQUES GRAPHQL");
    println!("   - Introspection query pour récupérer le schéma complet");
    println!("   - Requêtes profondément imbriquées (DoS par complexité)");
    println!("   - Mutations non documentées / champs cachés");
    println!();
    println!("7. ATTAQUES AVANCÉES");
    println!("   - Cache poisoning (en-têtes X-Forwarded-Host, X-Forwarded-Scheme)");
    println!("   - SSRF via paramètres d'URL (webhooks, callbacks, imports)");
    println!("   - Désérialisation insecure (JSON, YAML, pickle, Marshal)");
    println!("   - Race conditions (envoyer la même requête simultanément)");
    println!("   - HTTP Request Smuggling (CL/TE, TE.CL)");
    println!();
    println!("8. OUTILS COMPLÉMENTAIRES");
    println!("   - mitmproxy / Burp Suite / Charles Proxy");
    println!("   - ffuf / gobuster / dirsearch (bruteforce de chemins)");
    println!("   - Postman / Insomnia (exploration manuelle)");
    println!("   - jq / gron (parsing JSON)");
    println!("   - gitleaks / truffleHog (détection de secrets)");
}
