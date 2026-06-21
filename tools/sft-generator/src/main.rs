//! SoulSystem SFT dataset generator.
//!
//! Walks the workspace, parses every `.rs` file with `syn`, and emits supervised
//! fine-tuning pairs (chat `messages` format, one JSON object per line) that show
//! how to use the real public API: functions, structs, enums, traits, crate docs
//! and genuine tests.
//!
//! Usage:
//!   sft-generator [--root .] [--out sft_dataset.jsonl] [--augment 1]
//!                 [--limit N] [--sample sample.jsonl] [--stats-only]

mod extract;
mod model;
mod templates;

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use rayon::prelude::*;
use walkdir::WalkDir;

use extract::{extract_file, CrateMap};
use model::{ItemKind, Pair};

const SYSTEM: &str = "Tu es un assistant expert du codebase SoulSystem, un monorepo Rust d'agents autonomes (réseau neuronal SoulLink, entité autonome ReAct, écosystème AVID, framework SciRust). Tu réponds avec des explications claires et des exemples de code Rust corrects et idiomatiques, en t'appuyant sur l'API réelle des crates.";

struct Args {
    root: PathBuf,
    out: PathBuf,
    sample: Option<PathBuf>,
    sample_size: usize,
    limit: usize,
    augment: usize,
    stats_only: bool,
}

impl Args {
    fn parse() -> Args {
        let mut a = Args {
            root: PathBuf::from("."),
            out: PathBuf::from("sft_dataset.jsonl"),
            sample: None,
            sample_size: 80,
            limit: 0,
            augment: 1,
            stats_only: false,
        };
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--root" => a.root = PathBuf::from(it.next().unwrap_or_else(|| ".".into())),
                "--out" => {
                    a.out = PathBuf::from(it.next().unwrap_or_else(|| a.out.display().to_string()))
                }
                "--sample" => {
                    a.sample = Some(PathBuf::from(
                        it.next().unwrap_or_else(|| "sample.jsonl".into()),
                    ))
                }
                "--sample-size" => {
                    a.sample_size = it.next().and_then(|s| s.parse().ok()).unwrap_or(80)
                }
                "--limit" => a.limit = it.next().and_then(|s| s.parse().ok()).unwrap_or(0),
                "--augment" => {
                    a.augment = it.next().and_then(|s| s.parse().ok()).unwrap_or(1).max(1)
                }
                "--stats-only" => a.stats_only = true,
                other => eprintln!("argument ignoré : {other}"),
            }
        }
        a
    }
}

#[derive(Default)]
struct Stats {
    files_seen: usize,
    files_parsed: usize,
    files_failed: usize,
    crates: usize,
    items: HashMap<&'static str, usize>,
    pairs_by_source: HashMap<&'static str, usize>,
    distinct_pairs: usize,
    augmented_pairs: usize,
}

fn main() {
    let args = Args::parse();
    let t0 = Instant::now();

    eprintln!("→ Cartographie des crates…");
    let crate_map = CrateMap::build(&args.root);
    eprintln!("  {} crates détectés", crate_map.len());

    eprintln!("→ Collecte des fichiers Rust…");
    let files = collect_rs_files(&args.root);
    eprintln!("  {} fichiers .rs", files.len());

    eprintln!("→ Parsing (parallèle)…");
    let extracts: Vec<_> = files
        .par_iter()
        .filter_map(|f| {
            let crate_name = crate_map.crate_for(f)?;
            extract_file(f, crate_name)
        })
        .collect();

    // Aggregate re-exports per crate (collected across that crate's files).
    let mut reexports: HashMap<String, HashSet<String>> = HashMap::new();
    let mut stats = Stats {
        files_seen: files.len(),
        files_parsed: extracts.len(),
        files_failed: files.len().saturating_sub(extracts.len()),
        crates: crate_map.len(),
        ..Default::default()
    };
    for ex in &extracts {
        let set = reexports.entry(ex.crate_name.clone()).or_default();
        for r in &ex.reexports {
            set.insert(r.clone());
        }
        for it in &ex.items {
            *stats.items.entry(kind_tag(&it.kind)).or_default() += 1;
        }
    }

    if args.stats_only {
        print_pre_stats(&stats);
        return;
    }

    // Generate, dedup, write.
    eprintln!("→ Génération des paires SFT (augment ×{})…", args.augment);
    let out_file = File::create(&args.out).expect("création du fichier de sortie");
    let mut writer = BufWriter::new(out_file);

    let mut sample_writer = args
        .sample
        .as_ref()
        .map(|p| BufWriter::new(File::create(p).expect("création du sample")));
    let mut sample_by_source: HashMap<&'static str, usize> = HashMap::new();
    let mut sample_total = 0usize;

    let mut seen: HashSet<u64> = HashSet::new();
    let mut written = 0usize;

    // Augmentation runs as outer rounds so coverage stays balanced: round 0 is
    // the canonical phrasing of every item, round 1 a second phrasing of every
    // item, etc. With a `--limit`, the final round is simply truncated.
    'outer: for k in 0..args.augment {
        for ex in &extracts {
            for it in &ex.items {
                for pair in templates::pairs_for(it) {
                    let user = paraphrase(&pair.user, k);
                    let mut hasher = DefaultHasher::new();
                    user.hash(&mut hasher);
                    if !seen.insert(hasher.finish()) {
                        continue;
                    }
                    let line = to_jsonl(&user, &pair, &ex.crate_name);
                    writer.write_all(line.as_bytes()).unwrap();
                    writer.write_all(b"\n").unwrap();

                    written += 1;
                    *stats.pairs_by_source.entry(pair.source).or_default() += 1;
                    if k > 0 {
                        stats.augmented_pairs += 1;
                    }

                    // Diverse, human-readable sample (canonical phrasing only).
                    if k == 0 {
                        if let Some(sw) = sample_writer.as_mut() {
                            let n = sample_by_source.entry(pair.source).or_default();
                            if *n < 4 && sample_total < args.sample_size {
                                sw.write_all(line.as_bytes()).unwrap();
                                sw.write_all(b"\n").unwrap();
                                *n += 1;
                                sample_total += 1;
                            }
                        }
                    }

                    if args.limit != 0 && written >= args.limit {
                        break 'outer;
                    }
                }
            }
        }
    }

    // Per-crate re-export listing pairs.
    if args.limit == 0 || written < args.limit {
        let mut crates: Vec<_> = reexports.iter().collect();
        crates.sort_by(|a, b| a.0.cmp(b.0));
        for (c, set) in crates {
            if set.is_empty() {
                continue;
            }
            let mut names: Vec<_> = set.iter().cloned().collect();
            names.sort();
            names.truncate(60);
            let user = format!(
                "Quels sont les principaux symboles exportés (ré-exports publics) du crate `{c}` ?"
            );
            let mut hasher = DefaultHasher::new();
            user.hash(&mut hasher);
            if !seen.insert(hasher.finish()) {
                continue;
            }
            let pair = Pair {
                user,
                assistant: format!(
                    "Le crate `{c}` ré-exporte notamment : {}.",
                    names
                        .iter()
                        .map(|n| format!("`{n}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                source: "crate.exports",
            };
            let line = to_jsonl(&pair.user, &pair, c);
            writer.write_all(line.as_bytes()).unwrap();
            writer.write_all(b"\n").unwrap();
            written += 1;
            *stats.pairs_by_source.entry("crate.exports").or_default() += 1;
            if args.limit != 0 && written >= args.limit {
                break;
            }
        }
    }

    writer.flush().unwrap();
    if let Some(mut sw) = sample_writer {
        sw.flush().unwrap();
    }
    stats.distinct_pairs = written;

    print_final_stats(&stats, &args, t0.elapsed().as_secs_f32());
}

fn collect_rs_files(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.extension().map(|x| x == "rs").unwrap_or(false))
        .filter(|p| {
            !p.components().any(|c| {
                let s = c.as_os_str();
                s == "target" || s == ".git"
            })
        })
        // Skip the generator's own sources.
        .filter(|p| !p.components().any(|c| c.as_os_str() == "sft-generator"))
        .collect()
}

fn kind_tag(k: &ItemKind) -> &'static str {
    match k {
        ItemKind::Function { .. } => "function",
        ItemKind::Struct { .. } => "struct",
        ItemKind::Enum { .. } => "enum",
        ItemKind::Trait { .. } => "trait",
        ItemKind::Test { .. } => "test",
        ItemKind::ModuleDoc => "module_doc",
    }
}

/// Serialize one example to a JSONL line in chat `messages` format.
fn to_jsonl(user: &str, pair: &Pair, crate_name: &str) -> String {
    let value = serde_json::json!({
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": user},
            {"role": "assistant", "content": pair.assistant},
        ],
        "meta": {
            "source": pair.source,
            "crate": crate_name,
        }
    });
    serde_json::to_string(&value).unwrap()
}

/// Instruction-diversity augmentation: rephrase the user turn while keeping the
/// grounded answer. `k == 0` is the canonical phrasing (premium set).
fn paraphrase(user: &str, k: usize) -> String {
    match k {
        0 => user.to_string(),
        1 => format!("J'ai une question sur SoulSystem. {user}"),
        2 => format!("Peux-tu m'aider ? {}", lower_first(user)),
        3 => format!("[SoulSystem] {user}"),
        4 => format!("{user} Donne un exemple de code."),
        5 => format!(
            "En tant que développeur sur SoulSystem, {}",
            lower_first(user)
        ),
        6 => format!(
            "Dans le cadre du monorepo SoulSystem : {}",
            lower_first(user)
        ),
        7 => format!("Je débute sur SoulSystem. {user}"),
        8 => format!("Rappelle-moi : {}", lower_first(user)),
        9 => format!("Pour ma documentation technique, {}", lower_first(user)),
        10 => format!("Question rapide — {}", lower_first(user)),
        // Beyond the distinct templates above, further rounds would collapse on
        // dedup; this keeps one extra deterministic phrasing.
        _ => format!("{user}\n\n(Réponds de façon concise et précise.)"),
    }
}

fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn print_pre_stats(s: &Stats) {
    eprintln!("\n──────── STATISTIQUES (extraction) ────────");
    println!("Crates                 : {}", s.crates);
    println!("Fichiers .rs vus       : {}", s.files_seen);
    println!("Fichiers parsés        : {}", s.files_parsed);
    println!("Échecs de parsing      : {}", s.files_failed);
    println!("\nÉléments extraits :");
    let mut items: Vec<_> = s.items.iter().collect();
    items.sort_by(|a, b| b.1.cmp(a.1));
    let mut total = 0;
    for (k, v) in &items {
        println!("  {:<14} : {}", k, v);
        total += **v;
    }
    println!("  {:<14} : {}", "TOTAL", total);
}

fn print_final_stats(s: &Stats, args: &Args, secs: f32) {
    print_pre_stats(s);
    println!("\n──────── PAIRES SFT GÉNÉRÉES ────────");
    let mut by: Vec<_> = s.pairs_by_source.iter().collect();
    by.sort_by(|a, b| b.1.cmp(a.1));
    for (k, v) in &by {
        println!("  {:<16} : {}", k, v);
    }
    println!("\n  Paires distinctes     : {}", s.distinct_pairs);
    println!("  Dont augmentées       : {}", s.augmented_pairs);
    println!("  Facteur augment       : ×{}", args.augment);
    println!("  Fichier de sortie     : {}", args.out.display());
    if let Some(sp) = &args.sample {
        println!("  Échantillon           : {}", sp.display());
    }
    println!("  Durée                 : {secs:.1}s");
}
