## 2025-05-15 - [Redundant HTML Parsing in avid-scout]
**Learning:** The `ScoutEngine::crawl` method was parsing the same HTML body multiple times (over 10 times) by passing the raw string to various extraction submodules. Each submodule was creating its own `RcDom`, leading to significant CPU and memory overhead.
**Action:** Refactor extraction modules to provide `_from_dom` variants. Parse the HTML once into an `RcDom` in the main `crawl` loop and share it across all modules. Ensure the `RcDom` (which is `!Send`) is scoped to avoid async thread-safety issues.

## 2025-05-16 - [Redundant Regex Compilation in avid-scout]
**Learning:** Functions like `compute_budget` and `extract_viewport` were recompiling the same regular expressions on every invocation. In Rust, `Regex::new` is expensive as it involves parsing and DFA construction. Caching these with `OnceLock` provides a massive performance boost (up to 83% for `compute_budget`).
**Action:** Always use `OnceLock<Regex>` for static patterns in high-frequency paths. Avoid helper functions that recompile regexes dynamically (like the old `tag_regex`).
<<<<<<< HEAD
>>>>>>> REPLACE
=======
>>>>>>> origin/bolt-optimize-scout-parsing-8154599854471222384
