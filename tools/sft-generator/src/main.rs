//! SoulSystem SFT dataset generator.
//!
//! Walks the workspace, parses every `.rs` file with `syn`, and emits supervised
//! fine-tuning pairs that show how to use the real public API: functions,
//! structs, enums, traits, crate docs, genuine tests, plus cross-crate scenarios.
//!
//! Output formats: chat `messages` (default), Alpaca `instruction/input/output`,
//! or both. One JSON object per line.
//!
//! Usage:
//!   sft-generator [--root .] [--out sft_dataset.jsonl] [--format messages|alpaca|both]
//!                 [--augment 1] [--limit N] [--sample sample.jsonl] [--stats-only]

mod extract;
mod model;
mod scenarios;
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

#[derive(Clone, Copy, PartialEq)]
enum Format {
    Messages,
    Alpaca,
    Both,
}

struct Args {
    root: PathBuf,
    out: PathBuf,
    sample: Option<PathBuf>,
    sample_size: usize,
    limit: usize,
    augment: usize,
    stats_only: bool,
    format: Format,
}

impl Args {
    fn parse() -> Args {
        let mut a = Args {
            root: PathBuf::from("."),
            out: PathBuf::from("sft_dataset.jsonl"),
            sample: None,
            sample_size: 150,
            limit: 0,
            augment: 1,
            stats_only: false,
            format: Format::Messages,
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
                    a.sample_size = it.next().and_then(|s| s.parse().ok()).unwrap_or(150)
                }
                "--limit" => a.limit = it.next().and_then(|s| s.parse().ok()).unwrap_or(0),
                "--augment" => {
                    a.augment = it.next().and_then(|s| s.parse().ok()).unwrap_or(1).max(1)
                }
                "--format" => {
                    a.format = match it.next().as_deref() {
                        Some("alpaca") => Format::Alpaca,
                        Some("both") => Format::Both,
                        _ => Format::Messages,
                    }
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

/// Owns the output writers and dedup/sample state; `emit` is the single funnel
/// through which every pair is written.
struct Sink {
    msg: Option<BufWriter<File>>,
    alpaca: Option<BufWriter<File>>,
    sample: Option<BufWriter<File>>,
    sample_by_source: HashMap<&'static str, usize>,
    sample_total: usize,
    sample_size: usize,
    seen: HashSet<u64>,
    written: usize,
    limit: usize,
    stats: Stats,
}

impl Sink {
    /// Emit one pair under paraphrase round `k`. Returns `false` once `--limit`
    /// is reached (caller should stop).
    fn emit(&mut self, k: usize, pair: &Pair, crate_name: &str) -> bool {
        let user = paraphrase(&pair.user, k);
        let mut hasher = DefaultHasher::new();
        user.hash(&mut hasher);
        if !self.seen.insert(hasher.finish()) {
            return true; // exact duplicate instruction — skip, keep going
        }
        if let Some(w) = self.msg.as_mut() {
            let line = to_messages_json(&user, &pair.assistant, pair.source, crate_name);
            w.write_all(line.as_bytes()).unwrap();
            w.write_all(b"\n").unwrap();
        }
        if let Some(w) = self.alpaca.as_mut() {
            let line = to_alpaca_json(&user, &pair.assistant, pair.source, crate_name);
            w.write_all(line.as_bytes()).unwrap();
            w.write_all(b"\n").unwrap();
        }
        self.written += 1;
        *self.stats.pairs_by_source.entry(pair.source).or_default() += 1;
        if k > 0 {
            self.stats.augmented_pairs += 1;
        }
        if k == 0 {
            if let Some(sw) = self.sample.as_mut() {
                let n = self.sample_by_source.entry(pair.source).or_default();
                if *n < 4 && self.sample_total < self.sample_size {
                    let line = to_messages_json(&user, &pair.assistant, pair.source, crate_name);
                    sw.write_all(line.as_bytes()).unwrap();
                    sw.write_all(b"\n").unwrap();
                    *n += 1;
                    self.sample_total += 1;
                }
            }
        }
        !(self.limit != 0 && self.written >= self.limit)
    }

    fn finish(mut self) -> Stats {
        for w in [&mut self.msg, &mut self.alpaca, &mut self.sample]
            .into_iter()
            .flatten()
        {
            w.flush().unwrap();
        }
        self.stats.distinct_pairs = self.written;
        self.stats
    }
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

    // Composition pairs (cross-crate scenarios + per-crate re-export listings).
    eprintln!("→ Construction des scénarios inter-crates…");
    let index = scenarios::Index::build(&extracts);
    let mut extra: Vec<(Pair, String)> = index.pairs();
    let scenario_count = extra.len();
    extra.extend(export_pairs(&reexports));
    eprintln!(
        "  {scenario_count} scénarios, {} listes d'exports",
        extra.len() - scenario_count
    );

    // Resolve output paths per format.
    let (msg_path, alpaca_path) = match args.format {
        Format::Messages => (Some(args.out.clone()), None),
        Format::Alpaca => (None, Some(args.out.clone())),
        Format::Both => (Some(args.out.clone()), Some(alpaca_sibling(&args.out))),
    };

    eprintln!(
        "→ Génération des paires SFT (format {}, augment ×{})…",
        format_label(args.format),
        args.augment
    );

    let mut sink = Sink {
        msg: msg_path.as_ref().map(create),
        alpaca: alpaca_path.as_ref().map(create),
        sample: args.sample.as_ref().map(create),
        sample_by_source: HashMap::new(),
        sample_total: 0,
        sample_size: args.sample_size,
        seen: HashSet::new(),
        written: 0,
        limit: args.limit,
        stats,
    };

    // Balanced round-based augmentation: round 0 is the canonical phrasing of
    // every pair, round 1 a second phrasing, etc. `--limit` truncates the tail.
    'outer: for k in 0..args.augment {
        for ex in &extracts {
            for it in &ex.items {
                for pair in templates::pairs_for(it) {
                    if !sink.emit(k, &pair, &ex.crate_name) {
                        break 'outer;
                    }
                }
            }
        }
        for (pair, crate_name) in &extra {
            if !sink.emit(k, pair, crate_name) {
                break 'outer;
            }
        }
    }

    let stats = sink.finish();
    print_final_stats(
        &stats,
        &args,
        &msg_path,
        &alpaca_path,
        t0.elapsed().as_secs_f32(),
    );
}

fn create(p: &PathBuf) -> BufWriter<File> {
    BufWriter::new(File::create(p).expect("création du fichier de sortie"))
}

fn alpaca_sibling(out: &Path) -> PathBuf {
    let s = out.to_string_lossy();
    match s.strip_suffix(".jsonl") {
        Some(stem) => PathBuf::from(format!("{stem}.alpaca.jsonl")),
        None => PathBuf::from(format!("{s}.alpaca.jsonl")),
    }
}

fn format_label(f: Format) -> &'static str {
    match f {
        Format::Messages => "messages",
        Format::Alpaca => "alpaca",
        Format::Both => "messages+alpaca",
    }
}

fn export_pairs(reexports: &HashMap<String, HashSet<String>>) -> Vec<(Pair, String)> {
    let mut crates: Vec<_> = reexports.iter().collect();
    crates.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = Vec::new();
    for (c, set) in crates {
        if set.is_empty() {
            continue;
        }
        let mut names: Vec<_> = set.iter().cloned().collect();
        names.sort();
        names.truncate(60);
        out.push((
            Pair {
                user: format!(
                    "Quels sont les principaux symboles exportés (ré-exports publics) du crate `{c}` ?"
                ),
                assistant: format!(
                    "Le crate `{c}` ré-exporte notamment : {}.",
                    names
                        .iter()
                        .map(|n| format!("`{n}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                source: "crate.exports",
            },
            c.clone(),
        ));
    }
    out
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

/// One JSONL line in chat `messages` format.
fn to_messages_json(user: &str, assistant: &str, source: &str, crate_name: &str) -> String {
    let value = serde_json::json!({
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": user},
            {"role": "assistant", "content": assistant},
        ],
        "meta": {"source": source, "crate": crate_name}
    });
    serde_json::to_string(&value).unwrap()
}

/// One JSONL line in Alpaca `instruction/input/output` format.
fn to_alpaca_json(user: &str, assistant: &str, source: &str, crate_name: &str) -> String {
    let value = serde_json::json!({
        "instruction": user,
        "input": "",
        "output": assistant,
        "system": SYSTEM,
        "meta": {"source": source, "crate": crate_name}
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

fn print_final_stats(
    s: &Stats,
    args: &Args,
    msg_path: &Option<PathBuf>,
    alpaca_path: &Option<PathBuf>,
    secs: f32,
) {
    print_pre_stats(s);
    println!("\n──────── PAIRES SFT GÉNÉRÉES ────────");
    let mut by: Vec<_> = s.pairs_by_source.iter().collect();
    by.sort_by(|a, b| b.1.cmp(a.1));
    for (k, v) in &by {
        println!("  {:<18} : {}", k, v);
    }
    println!("\n  Paires distinctes     : {}", s.distinct_pairs);
    println!("  Dont augmentées       : {}", s.augmented_pairs);
    println!("  Facteur augment       : ×{}", args.augment);
    println!("  Format                : {}", format_label(args.format));
    if let Some(p) = msg_path {
        println!("  Sortie (messages)     : {}", p.display());
    }
    if let Some(p) = alpaca_path {
        println!("  Sortie (alpaca)       : {}", p.display());
    }
    if let Some(sp) = &args.sample {
        println!("  Échantillon           : {}", sp.display());
    }
    println!("  Durée                 : {secs:.1}s");
}
