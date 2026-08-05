# Contributing to ocpp-types

Contributions are welcome — implementing new message types, fixing specification
inconsistencies, adding tests, or improving documentation.

## Workspace layout

- `crates/ocpp-types` — the published library crate. `no_std`, uses `heapless` for
  stack-allocated strings/vecs instead of `String`/`Vec`.
- `crates/ocpp-codegen` — a dev-time binary (not published) that generates the types in
  `ocpp-types` from the JSON schemas in `schemas/`.
- `schemas/ocpp1.6j`, `schemas/ocpp2.0.1`, `schemas/ocpp2.1` — the JSON schema source of truth
  for each protocol version.

## Types are generated, not hand-written

If a message type is wrong or missing, fix the generator (`ocpp-codegen`) or the schema, not the
generated code directly — every generated file says as much at the top, and regenerating
overwrites hand-edits anyway. After changing a schema or the generator, regenerate and commit the
result:

```sh
./scripts/generate.sh
```

CI regenerates and diffs on every push, so a PR fails if the committed output doesn't match what
the schemas and generator would actually produce.

## Test-Driven Development

This repo follows TDD. For any new behavior or bug fix:

1. Write a failing test first (in `ocpp-codegen` for generator logic, or against generated
   `ocpp-types` output for type/serde behavior).
2. Implement the minimal change to make it pass.
3. Refactor with the test suite green.

Don't write implementation code without a preceding failing test demonstrating the need for it.

## `ocpp-types` constraints

- Must stay `no_std`. Do not introduce `std`-only dependencies or APIs.
- Use `heapless::String<N>` / `heapless::Vec<T, N>` for anything the schema gives a max
  length/count for, sized to the OCPP spec's stated limits — not `alloc`/`std` collections.
- Types should implement `serde::Serialize`/`Deserialize`.
- Keep versions separated (`v16`, `v201`, `v21` modules) since the same message name can differ
  in shape across OCPP versions.

## Commands

```sh
cargo build --workspace
cargo test --workspace
cargo build -p ocpp-types --features alloc
cargo test -p ocpp-types --features serde
cargo test -p ocpp-types --features alloc
cargo test -p ocpp-types --features serde,alloc
cargo run -p ocpp-codegen -- <input-schema.json> <output.rs>
```

CI (`.github/workflows/ci.yml`) runs all of the above, plus the generated-types freshness check,
on every push and pull request.

## Submitting a change

1. Open an issue first for anything non-trivial (new message support, behavioral changes) to
   confirm the approach before investing time.
2. Keep PRs focused — one logical change per PR.
3. Make sure `cargo test --workspace` and `./scripts/generate.sh` (with no resulting diff) pass
   locally before opening a PR.
4. Add a `CHANGELOG.md` entry under `[Unreleased]` describing the change.
