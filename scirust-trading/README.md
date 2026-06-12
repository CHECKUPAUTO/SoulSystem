# scirust-trading

Trading crates extracted (no history) from the SciRust monorepo
(https://github.com/CHECKUPAUTO/scirust) into a standalone workspace.
No dependency on the SciRust deep-learning core.

Crates: core, engine, observer, persistence, news, monitor.

Note: imported as-is. scirust-trading-engine/src/shadow.rs carries a
pre-existing structural corruption (missing struct/fn declarations) and does
not compile yet; to be fixed here.
