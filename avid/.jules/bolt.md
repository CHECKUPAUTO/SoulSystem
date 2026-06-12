## 2025-05-15 - [Redundant HTML Parsing in avid-scout]
**Learning:** The `ScoutEngine::crawl` method was parsing the same HTML body multiple times (over 10 times) by passing the raw string to various extraction submodules. Each submodule was creating its own `RcDom`, leading to significant CPU and memory overhead.
**Action:** Refactor extraction modules to provide `_from_dom` variants. Parse the HTML once into an `RcDom` in the main `crawl` loop and share it across all modules. Ensure the `RcDom` (which is `!Send`) is scoped to avoid async thread-safety issues.
<<<<<<< HEAD
>>>>>>> REPLACE
=======
>>>>>>> origin/bolt-optimize-scout-parsing-8154599854471222384
