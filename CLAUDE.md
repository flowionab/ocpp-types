# ocpp-types workspace

Rust workspace implementing the Open Charge Point Protocol (OCPP) message model
for versions 1.6J, 2.0.1, and 2.1.

## Workspace layout

- `crates/ocpp-types` — the library crate. `no_std`, uses `heapless` for
  stack-allocated strings/vecs instead of `String`/`Vec`. This is the crate
  that gets published and consumed by downstream projects.
- `crates/ocpp-codegen` — a binary crate (not published, `publish = false`)
  that generates the Rust types in `ocpp-types` from the JSON schemas in
  `schemas/`. It is a dev-time tool, not a runtime dependency.
- `schemas/ocpp1.6j`, `schemas/ocpp2.0.1`, `schemas/ocpp2.1` — the JSON
  schema source of truth for each protocol version, one file per
  request/response message.

## Workflow

Types in `ocpp-types` are generated, not hand-written. If a message type is
wrong or missing, fix the generator (`ocpp-codegen`) rather than editing
generated code directly, so regeneration doesn't lose the fix.

## Test-Driven Development

This repo follows TDD. For any new behavior or bug fix:

1. Write a failing test first (in `ocpp-codegen` for generator logic, or
   against generated `ocpp-types` output for type/serde behavior).
2. Implement the minimal change to make it pass.
3. Refactor with the test suite green.

Don't write implementation code without a preceding failing test demonstrating
the need for it.

## `ocpp-types` constraints

- Must stay `no_std`. Do not introduce `std`-only dependencies or APIs.
- Use `heapless::String<N>` / `heapless::Vec<T, N>` for anything the schema
  gives a max length/count for, sized to the OCPP spec's stated limits — not
  `alloc`/`std` collections.
- Types should implement `serde::Serialize`/`Deserialize`.
- Keep versions separated (e.g. `v16`, `v201`, `v21` modules) since the same
  message name can differ in shape across OCPP versions.

## Commands

```sh
cargo build --workspace
cargo test --workspace
cargo run -p ocpp-codegen -- <input-schema.json> <output.rs>
```
