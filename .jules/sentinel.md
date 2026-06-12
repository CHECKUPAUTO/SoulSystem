## 2025-05-15 - Headless Browser JavaScript Injection
**Vulnerability:** manual construction of JavaScript strings using naive `.replace('\'', "\\'")` for CSS selectors and input values.
**Learning:** Naive escaping can be bypassed (e.g. with backslashes) and is prone to errors. `chromiumoxide` provides native CDP methods for interaction that bypass `page.evaluate()` entirely for many operations.
**Prevention:** Prefer native CDP methods like `element.click()` and `element.type_str()`. When JavaScript injection is unavoidable (e.g. clearing an input field), use `serde_json::to_string()` to safely escape strings as JavaScript literals.
