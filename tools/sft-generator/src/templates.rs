//! Turn structured [`ApiItem`]s into grounded SFT [`Pair`]s.
//!
//! Each item yields several genuinely distinct angles (usage, explanation,
//! signature, error handling, async, imports, construction, `match`, trait
//! implementation, real tests). Responses reuse the *actual* signatures, docs,
//! fields and variants pulled from source, so the dataset teaches the real API.

use crate::model::{ApiItem, ItemKind, Pair, Param};

/// Produce all training pairs for a single item.
pub fn pairs_for(item: &ApiItem) -> Vec<Pair> {
    match &item.kind {
        ItemKind::Function { .. } => function_pairs(item),
        ItemKind::Struct { .. } => struct_pairs(item),
        ItemKind::Enum { .. } => enum_pairs(item),
        ItemKind::Trait { .. } => trait_pairs(item),
        ItemKind::Test { .. } => test_pairs(item),
        ItemKind::ModuleDoc => module_pairs(item),
    }
}

// ─────────────────────────── functions / methods ───────────────────────────

fn function_pairs(item: &ApiItem) -> Vec<Pair> {
    let ItemKind::Function {
        is_async,
        receiver,
        params,
        ret,
        parent,
        signature,
        ..
    } = &item.kind
    else {
        return Vec::new();
    };

    let c = &item.crate_name;
    let name = &item.name;
    let is_result = ret.as_deref().is_some_and(|r| r.starts_with("Result"));
    let import = import_path(item);
    let example = build_example(
        name,
        parent,
        receiver.as_deref(),
        params,
        *is_async,
        is_result,
    );
    let summary = item
        .doc_summary()
        .map(|s| format!(" — {s}."))
        .unwrap_or_default();
    let where_ = match parent {
        Some(p) => format!(" sur le type `{p}`"),
        None => String::new(),
    };

    let mut pairs = Vec::new();

    // 1) Usage.
    pairs.push(Pair {
        user: format!("Dans le crate `{c}`, comment utiliser `{name}`{where_} ? Donne la signature et un exemple d'appel."),
        assistant: format!(
            "`{name}`{summary}\n\nSignature :\n```rust\n{signature}\n```\n\nExemple d'utilisation :\n```rust\nuse {import};\n\n{example}\n```"
        ),
        source: "fn.usage",
    });

    // 2) Explanation of params / return.
    pairs.push(Pair {
        user: format!(
            "Explique ce que fait `{name}` du crate `{c}` : ses paramètres et sa valeur de retour."
        ),
        assistant: explain_fn(item, params, ret, receiver.as_deref(), *is_async),
        source: "fn.explain",
    });

    // 3) Exact signature.
    let mod_note = if item.module_path.is_empty() {
        String::new()
    } else {
        format!(" (module `{}`)", item.module_path)
    };
    pairs.push(Pair {
        user: format!("Quelle est la signature exacte de `{name}` dans le crate `{c}`{mod_note} ?"),
        assistant: format!(
            "```rust\n{signature}\n```{}",
            item.doc_summary()
                .map(|s| format!("\n\nRôle : {s}."))
                .unwrap_or_default()
        ),
        source: "fn.signature",
    });

    // 4) Error handling (only when it returns a Result).
    if is_result {
        let ret_s = ret.clone().unwrap_or_default();
        pairs.push(Pair {
            user: format!("Comment gérer les erreurs renvoyées par `{name}` ({c}) ?"),
            assistant: format!(
                "`{name}` renvoie `{ret_s}`, il faut donc traiter le `Result`.\n\n\
                 Propagation avec `?` :\n```rust\nlet valeur = {call}?;\n```\n\n\
                 Ou gestion explicite :\n```rust\nmatch {call} {{\n    Ok(valeur) => {{ /* succès */ }}\n    Err(e) => eprintln!(\"échec de {name}: {{e}}\"),\n}}\n```",
                call = raw_call(name, parent, receiver.as_deref(), params, *is_async)
            ),
            source: "fn.errors",
        });
    }

    // 5) Async usage.
    if *is_async {
        pairs.push(Pair {
            user: format!("`{name}` ({c}) est-elle asynchrone ? Comment l'appeler correctement ?"),
            assistant: format!(
                "Oui, `{name}` est `async` : il faut l'attendre avec `.await` dans un contexte asynchrone (runtime Tokio).\n\n```rust\n{example}\n```"
            ),
            source: "fn.async",
        });
    }

    // 6) Import path.
    pairs.push(Pair {
        user: format!("Quel `use` faut-il écrire pour accéder à `{name}` du crate `{c}` ?"),
        assistant: format!(
            "```rust\nuse {import};\n```\n\nDans le code, le nom du crate s'écrit avec des underscores (`{}`), même si le crate se nomme `{c}`. Certains crates ré-exportent ce symbole à la racine, ce qui peut raccourcir le chemin.",
            crate_ident(c)
        ),
        source: "fn.import",
    });

    // 7) Where is it defined.
    pairs.push(location_pair(item, "la fonction"));

    pairs
}

fn explain_fn(
    item: &ApiItem,
    params: &[Param],
    ret: &Option<String>,
    receiver: Option<&str>,
    is_async: bool,
) -> String {
    let mut s = String::new();
    if let Some(doc) = &item.doc {
        s.push_str(doc.trim());
        s.push_str("\n\n");
    }
    if let Some(r) = receiver {
        s.push_str(&format!("- Récepteur : `{r}`\n"));
    }
    if params.is_empty() {
        s.push_str("- Paramètres : aucun\n");
    } else {
        s.push_str("- Paramètres :\n");
        for p in params {
            s.push_str(&format!("    - `{}` : `{}`\n", p.name, p.ty));
        }
    }
    match ret {
        Some(r) => s.push_str(&format!("- Retour : `{r}`\n")),
        None => s.push_str("- Retour : `()` (rien)\n"),
    }
    if is_async {
        s.push_str("- Fonction `async` : appelez-la avec `.await`.\n");
    }
    s.trim_end().to_string()
}

// ─────────────────────────────── structs ───────────────────────────────────

fn struct_pairs(item: &ApiItem) -> Vec<Pair> {
    let ItemKind::Struct {
        fields,
        is_tuple,
        is_unit,
        derives,
    } = &item.kind
    else {
        return Vec::new();
    };
    let c = &item.crate_name;
    let name = &item.name;
    let import = import_path(item);
    let has_default = derives.iter().any(|d| d == "Default");
    let summary = item
        .doc_summary()
        .map(|s| format!(" — {s}."))
        .unwrap_or_default();

    let mut pairs = Vec::new();

    // 1) Construction / usage.
    let construct = if *is_unit {
        format!("let valeur = {name};")
    } else if *is_tuple || fields.is_empty() {
        if has_default {
            format!("let valeur = {name}::default();")
        } else {
            format!("let valeur = {name}(/* … */);")
        }
    } else {
        let mut lit = format!("let valeur = {name} {{\n");
        for f in fields {
            lit.push_str(&format!("    {}: {},\n", f.name, lit_for(&f.ty)));
        }
        lit.push_str("};");
        if has_default {
            lit.push_str(&format!(
                "\n// ou, en s'appuyant sur Default :\nlet valeur = {name}::default();"
            ));
        }
        lit
    };
    pairs.push(Pair {
        user: format!("Comment construire et utiliser la struct `{name}` du crate `{c}` ?"),
        assistant: format!("`{name}`{summary}\n\n```rust\nuse {import};\n\n{construct}\n```"),
        source: "struct.usage",
    });

    // 2) Purpose + fields.
    pairs.push(Pair {
        user: format!("À quoi sert la struct `{name}` ({c}) et quels sont ses champs ?"),
        assistant: describe_struct(item, fields),
        source: "struct.explain",
    });

    // 3) Fields listing (only when there are named public fields).
    if !fields.is_empty() && !*is_tuple {
        let mut listing = String::from("Champs publics :\n");
        for f in fields {
            listing.push_str(&format!("- `{}` : `{}`\n", f.name, f.ty));
        }
        pairs.push(Pair {
            user: format!("Quels sont les champs publics de `{name}` dans le crate `{c}` ?"),
            assistant: listing.trim_end().to_string(),
            source: "struct.fields",
        });
    }

    // 4) Derives.
    if !derives.is_empty() {
        pairs.push(Pair {
            user: format!("Quels traits la struct `{name}` ({c}) dérive-t-elle et qu'est-ce que cela permet ?"),
            assistant: describe_derives(name, derives),
            source: "struct.derives",
        });
    }

    pairs.push(location_pair(item, "la struct"));

    pairs
}

fn describe_struct(item: &ApiItem, fields: &[crate::model::Field]) -> String {
    let mut s = String::new();
    if let Some(doc) = &item.doc {
        s.push_str(doc.trim());
        s.push_str("\n\n");
    }
    if fields.is_empty() {
        s.push_str("Cette struct n'expose pas de champ public (construite via ses méthodes).");
    } else {
        s.push_str("Champs :\n");
        for f in fields {
            s.push_str(&format!("- `{}` : `{}`\n", f.name, f.ty));
        }
    }
    s.trim_end().to_string()
}

// ──────────────────────────────── enums ────────────────────────────────────

fn enum_pairs(item: &ApiItem) -> Vec<Pair> {
    let ItemKind::Enum { variants, derives } = &item.kind else {
        return Vec::new();
    };
    let c = &item.crate_name;
    let name = &item.name;
    let import = import_path(item);
    let is_error = name.ends_with("Error") || derives.iter().any(|d| d == "Error");

    let mut match_arms = String::new();
    for v in variants {
        match_arms.push_str(&format!("    {name}::{v} => {{ /* … */ }}\n"));
    }

    let mut pairs = Vec::new();

    // 1) Variants + match.
    pairs.push(Pair {
        user: format!("Quelles sont les variantes de l'enum `{name}` ({c}) et comment faire un `match` dessus ?"),
        assistant: format!(
            "Variantes : {}.\n\n```rust\nuse {import};\n\nmatch valeur {{\n{match_arms}}}\n```",
            variants
                .iter()
                .map(|v| format!("`{v}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        source: "enum.usage",
    });

    // 2) Purpose.
    let mut explain = String::new();
    if let Some(doc) = &item.doc {
        explain.push_str(doc.trim());
        explain.push_str("\n\n");
    }
    explain.push_str(&format!(
        "L'enum `{name}` possède {} variante(s) : {}.",
        variants.len(),
        variants
            .iter()
            .map(|v| format!("`{v}`"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    pairs.push(Pair {
        user: format!("À quoi sert l'enum `{name}` du crate `{c}` ?"),
        assistant: explain,
        source: "enum.explain",
    });

    // 3) Error handling for error enums.
    if is_error {
        pairs.push(Pair {
            user: format!("Comment gérer l'erreur `{name}` du crate `{c}` ?"),
            assistant: format!(
                "`{name}` est un type d'erreur ; vous pouvez la propager avec `?` ou discriminer ses variantes :\n\n```rust\nuse {import};\n\nmatch resultat {{\n    Ok(v) => {{ /* … */ }}\n{}}}\n```",
                variants
                    .iter()
                    .map(|v| format!("    Err({name}::{v}) => {{ /* traiter {v} */ }}\n"))
                    .collect::<String>()
            ),
            source: "enum.errors",
        });
    }

    pairs.push(location_pair(item, "l'enum"));

    pairs
}

// ──────────────────────────────── traits ───────────────────────────────────

fn trait_pairs(item: &ApiItem) -> Vec<Pair> {
    let ItemKind::Trait {
        methods,
        signatures,
    } = &item.kind
    else {
        return Vec::new();
    };
    let c = &item.crate_name;
    let name = &item.name;
    let import = import_path(item);

    let mut pairs = Vec::new();

    // 1) What it requires.
    let mut req = String::new();
    if let Some(doc) = &item.doc {
        req.push_str(doc.trim());
        req.push_str("\n\n");
    }
    if methods.is_empty() {
        req.push_str(
            "Ce trait ne déclare pas de méthode nommée (marqueur ou méthodes par défaut).",
        );
    } else {
        req.push_str("Méthodes à implémenter :\n");
        for m in methods {
            req.push_str(&format!("- `{m}`\n"));
        }
    }
    pairs.push(Pair {
        user: format!("Que faut-il pour implémenter le trait `{name}` du crate `{c}` ?"),
        assistant: req.trim_end().to_string(),
        source: "trait.explain",
    });

    // 2) Implementation skeleton (using the real method signatures).
    let body = if signatures.is_empty() {
        "    // trait marqueur : rien à implémenter\n".to_string()
    } else {
        signatures
            .iter()
            .map(|sig| format!("    {sig} {{\n        todo!()\n    }}\n"))
            .collect::<String>()
    };
    pairs.push(Pair {
        user: format!(
            "Montre un squelette d'implémentation du trait `{name}` ({c}) pour un type maison."
        ),
        assistant: format!(
            "```rust\nuse {import};\n\nstruct MonType;\n\nimpl {name} for MonType {{\n{body}}}\n```"
        ),
        source: "trait.impl",
    });

    // 3) As a bound.
    pairs.push(Pair {
        user: format!("Comment utiliser `{name}` ({c}) comme borne de trait dans une fonction générique ?"),
        assistant: format!(
            "```rust\nuse {import};\n\nfn traiter<T: {name}>(valeur: T) {{\n    // `valeur` respecte le contrat `{name}`\n}}\n```"
        ),
        source: "trait.bound",
    });

    pairs.push(location_pair(item, "le trait"));

    pairs
}

// ──────────────────────────────── tests ────────────────────────────────────

fn test_pairs(item: &ApiItem) -> Vec<Pair> {
    let ItemKind::Test { body, .. } = &item.kind else {
        return Vec::new();
    };
    let c = &item.crate_name;
    let loc = if item.module_path.is_empty() {
        format!("crate `{c}`")
    } else {
        format!("crate `{c}`, module `{}`", item.module_path)
    };
    vec![Pair {
        user: format!("Montre un exemple concret et fonctionnel tiré du {loc}, illustrant le comportement de `{}`.", item.name),
        assistant: format!(
            "Voici un test réel du crate, qui illustre l'usage attendu :\n\n```rust\n{body}\n```"
        ),
        source: "test.usage",
    }]
}

// ─────────────────────────── module / crate docs ───────────────────────────

fn module_pairs(item: &ApiItem) -> Vec<Pair> {
    let c = &item.crate_name;
    let target = if item.module_path.is_empty() {
        format!("crate `{c}`")
    } else {
        format!("module `{}::{}`", c, item.module_path)
    };
    let doc = item.doc.clone().unwrap_or_default();

    let mut pairs = vec![Pair {
        user: format!("À quoi sert le {target} dans SoulSystem ?"),
        assistant: doc.trim().to_string(),
        source: "crate.overview",
    }];

    if let Some(example) = item.doc_examples.first() {
        pairs.push(Pair {
            user: format!("Donne un exemple d'utilisation du {target}."),
            assistant: format!("```rust\n{}\n```", example.trim_end()),
            source: "crate.example",
        });
    }

    pairs
}

// ─────────────────────────────── helpers ───────────────────────────────────

/// Crate name as it appears *in code*: cargo allows `-` in package names but
/// the Rust path uses `_` (e.g. `soul-memory` → `soul_memory`).
fn crate_ident(name: &str) -> String {
    name.replace('-', "_")
}

/// A codebase-navigation pair: "where is X defined?". Grounded in the real file
/// path and module, this is exactly what a repo assistant is asked.
fn location_pair(item: &ApiItem, kind_label: &str) -> Pair {
    let c = &item.crate_name;
    let file = item.file.trim_start_matches("./");
    let module = if item.module_path.is_empty() {
        String::new()
    } else {
        format!(", module `{}`", item.module_path)
    };
    Pair {
        user: format!(
            "Où est défini(e) {kind_label} `{}` dans SoulSystem (crate, module, fichier) ?",
            item.name
        ),
        assistant: format!(
            "On trouve {kind_label} `{}` dans le crate `{c}`{module}.\n\n- Fichier : `{file}`\n- Import : `use {};`",
            item.name,
            import_path(item)
        ),
        source: "nav.location",
    }
}

/// The `use` path for an item: the parent type for methods, otherwise the item.
fn import_path(item: &ApiItem) -> String {
    let c = crate_ident(&item.crate_name);
    let m = &item.module_path;
    let owner = match &item.kind {
        ItemKind::Function {
            parent: Some(p), ..
        } => p.clone(),
        _ => item.name.clone(),
    };
    if m.is_empty() {
        format!("{c}::{owner}")
    } else {
        format!("{c}::{m}::{owner}")
    }
}

fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    if out.is_empty() {
        "valeur".to_string()
    } else {
        out
    }
}

/// A bare call expression (no `let`, no wrapper), used in error-handling snippets.
fn raw_call(
    name: &str,
    parent: &Option<String>,
    receiver: Option<&str>,
    params: &[Param],
    is_async: bool,
) -> String {
    let args = params
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let awaitp = if is_async { ".await" } else { "" };
    let core = match (parent, receiver) {
        (Some(p), None) => format!("{p}::{name}({args})"),
        (Some(p), Some(_)) => format!("{}.{name}({args})", snake(p)),
        _ => format!("{name}({args})"),
    };
    format!("{core}{awaitp}")
}

/// A full, idiomatic usage example (wrapped in an async/Result context if needed).
fn build_example(
    name: &str,
    parent: &Option<String>,
    receiver: Option<&str>,
    params: &[Param],
    is_async: bool,
    is_result: bool,
) -> String {
    let args = params
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let awaitp = if is_async { ".await" } else { "" };
    let q = if is_result { "?" } else { "" };

    let mut lines: Vec<String> = Vec::new();

    // Parameter reminder, so the reader knows what to plug in.
    if !params.is_empty() {
        for p in params {
            lines.push(format!("// {} : {}", p.name, p.ty));
        }
    }

    let core = match (parent, receiver) {
        (Some(p), Some(_)) => {
            let var = snake(p);
            lines.insert(
                0,
                format!("// `{var}` : une instance de `{p}` déjà construite"),
            );
            format!("{var}.{name}({args})")
        }
        (Some(p), None) => format!("{p}::{name}({args})"),
        _ => format!("{name}({args})"),
    };
    lines.push(format!("let resultat = {core}{awaitp}{q};"));

    let body = lines.join("\n");
    wrap_example(&body, is_async, is_result)
}

fn wrap_example(body: &str, is_async: bool, is_result: bool) -> String {
    if is_result {
        let kw = if is_async { "async fn" } else { "fn" };
        format!(
            "{kw} exemple() -> anyhow::Result<()> {{\n{}\n    Ok(())\n}}",
            indent4(body)
        )
    } else if is_async {
        format!("async fn exemple() {{\n{}\n}}", indent4(body))
    } else {
        body.to_string()
    }
}

fn indent4(s: &str) -> String {
    s.lines()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("    {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn describe_derives(name: &str, derives: &[String]) -> String {
    let mut s = format!("`{name}` dérive : {}.\n\n", derives.join(", "));
    for d in derives {
        let note = match d.as_str() {
            "Debug" => "affichage de débogage via `{:?}`",
            "Clone" => "duplication par `.clone()`",
            "Copy" => "copie implicite (sémantique de valeur)",
            "Default" => "construction par défaut via `::default()`",
            "PartialEq" | "Eq" => "comparaison d'égalité avec `==`",
            "Hash" => "utilisation comme clé de `HashMap`/`HashSet`",
            "Serialize" => "sérialisation (serde) vers JSON, etc.",
            "Deserialize" => "désérialisation (serde) depuis JSON, etc.",
            "PartialOrd" | "Ord" => "comparaison d'ordre (`<`, `>`, tri)",
            "Error" => "type d'erreur standard (`std::error::Error`)",
            _ => continue,
        };
        s.push_str(&format!("- `{d}` : {note}\n"));
    }
    s.trim_end().to_string()
}

fn lit_for(ty: &str) -> String {
    let t = ty.trim();
    if t == "String" || t.ends_with("::String") {
        "String::new()".into()
    } else if t == "&str" || t == "& str" {
        "\"...\"".into()
    } else if is_int(t) {
        "0".into()
    } else if t == "f32" || t == "f64" {
        "0.0".into()
    } else if t == "bool" {
        "false".into()
    } else if t == "char" {
        "'a'".into()
    } else if t.starts_with("Option") {
        "None".into()
    } else if t.starts_with("Vec") {
        "Vec::new()".into()
    } else {
        format!("Default::default() /* {t} */")
    }
}

fn is_int(t: &str) -> bool {
    matches!(
        t,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
    )
}
