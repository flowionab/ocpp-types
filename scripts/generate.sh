#!/usr/bin/env bash
# Regenerates crates/ocpp-types/src/{v16,v201,v21} from schemas/. Run this
# after changing a schema or the generator, and commit the result -- CI
# checks that this script's output matches what's committed.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

cargo build -p ocpp-codegen

BIN="target/debug/ocpp-codegen"

# `--csv` supplies the spec's value sets for fields the JSON schemas type as
# bare strings (standardized component/variable names, security events,
# reason codes, units). See crates/ocpp-codegen/src/standard.rs.
"$BIN" schemas/ocpp1.6j crates/ocpp-types/src/v16 --csv csv/ocpp1.6j
"$BIN" schemas/ocpp2.0.1 crates/ocpp-types/src/v201 --csv csv/ocpp2.0.1
"$BIN" schemas/ocpp2.1 crates/ocpp-types/src/v21 --csv csv/ocpp2.1
