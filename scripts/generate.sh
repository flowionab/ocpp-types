#!/usr/bin/env bash
# Regenerates crates/ocpp-types/src/{v16,v201,v21} from schemas/. Run this
# after changing a schema or the generator, and commit the result -- CI
# checks that this script's output matches what's committed.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

cargo build -p ocpp-codegen

BIN="target/debug/ocpp-codegen"

"$BIN" schemas/ocpp1.6j crates/ocpp-types/src/v16
"$BIN" schemas/ocpp2.0.1 crates/ocpp-types/src/v201
"$BIN" schemas/ocpp2.1 crates/ocpp-types/src/v21
