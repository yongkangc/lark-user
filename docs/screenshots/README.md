# README screenshots

These PNGs present selected fields from actual CLI output, captured in Chromium.
They are documentation illustrations, not an application GUI or live Lark proof.
The capture runs the compiled binary against the deterministic synthetic fixture
in a separate temporary store. No cookies, HARs or enterprise content are used.
The adjacent JSON files record the displayed commands and fields.

Rebuild using Node 22+, Bun and Rust:

```sh
cargo build --locked
demo_tools=$(mktemp -d)
bun add --cwd "$demo_tools" playwright@1.64.0
bun "$demo_tools/node_modules/playwright/cli.js" install chromium
PLAYWRIGHT_MODULE="$demo_tools/node_modules/playwright/index.mjs" \
  node docs/screenshots/capture.mjs target/debug/lark-user
```

The dependency stays outside the Rust repository. Use the committed PNGs for
lasting README images; PR descriptions use freshly uploaded 72-hour Litterbox copies.
