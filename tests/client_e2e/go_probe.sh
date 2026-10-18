#!/bin/sh
# Fast conversion-only check of real Codex / Claude Code requests against the Go validators.
set -e
cd "$(dirname "$0")"
OUT=${GO_PROBE_OUT:-/tmp/go_probe}
rm -rf "$OUT"
node --input-type=module -e "import { GO_MODELS } from './go_mode.mjs'; console.log(JSON.stringify(GO_MODELS))" > "$OUT.models.json"
(cd ../.. && GO_MODELS_FILE="$OUT.models.json" GO_PROBE_OUT="$OUT" RUST_MIN_STACK=16777216 cargo test -q -p aether-ai-formats --test go_real_clients >/dev/null)
node go_validate.mjs "$OUT"
