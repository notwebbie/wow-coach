# Browser bindings

The thin layer between `wow-coach-core` and the web client. It holds no rules:
everything here parses, reshapes or forwards.

## Why this is a separate crate

The core must not depend on `wasm-bindgen`. Dropping `chrono`'s default
features took the core's wasm dependency tree from 57 entries to 35 by removing
the JavaScript shim, and the `wasm` CI job fails if `wasm-bindgen` or `js-sys`
reappear in `cargo tree -p wow-coach-core`. Putting the bindings in the core
would trip that guard, correctly. They live here instead.

## What crosses the boundary

Two shapes, for two different reasons.

**Structured data** for the roster and the roles — the page renders those, and
wants numbers it can lay out, sort and put in a form.

**A finished sentence** for the recommendation. The wording *is* the product
there, and `docs/ARCHITECTURE.md` says frontends are presentation only and
neither re-implements a rule. If the web client assembled that sentence from
parts, the desktop client would assemble a slightly different one, and the two
would drift. `Suggestion::headline()` builds it once, in the core.

Everything is passed as a JSON string for `JSON.parse` rather than through
`serde-wasm-bindgen`. A roster is tens of characters, the cost is invisible,
and it keeps a dependency out of the bundle.
