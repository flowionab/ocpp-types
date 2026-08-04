# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2]

### Fixed

- `alloc` feature: spec-bounded array fields above 64 items (e.g.
  `ChargingSchedule.chargingSchedulePeriod`, `maxItems: 1024`) still rendered as an inline
  `heapless::Vec<T, N>` even in the `alloc` build, instead of switching to a heap-allocated
  `alloc::Vec<T>` the way genuinely unbounded fields already did. This made 2.0.1/2.1's
  `ChargingProfile` ~80MB by value, easily overflowing the stack for any caller that holds one
  across an `.await` in a boxed future (e.g. an OCPP client's `RequestStartTransactionRequest`
  handler). `ocpp-codegen` now switches any bounded `Vec`/array field above that size threshold to
  `alloc::Vec<T>` under the `alloc` feature, cutting `ChargingProfile` from ~80MB down to ~55KB.
  The `no_std`/non-`alloc` build is unaffected — those fields stay `heapless`, sized to the spec's
  stated limit, as intended for stack-only embedded use.

## [0.1.1]

### Fixed

- 1.6J: anonymous inline JSON-schema enums (`status`, `type`, `unit`) were named purely from their
  property name, so distinct messages sharing a property name silently collapsed onto one shared
  generated type — most notably `status`, where 14 response types (`SendLocalListResponse`,
  `ReserveNowResponse`, `BootNotificationResponse`, and 11 others) each define their own distinct
  status values but all landed on `IdTagInfo`'s 5-value status enum. `ocpp-codegen` now detects
  when a property name means different things across messages and qualifies the generated type
  with its owning message name instead, so each response gets its own correctly-shaped enum.

## [0.1.0]

### Added

- Request/response message types for OCPP 1.6J, 2.0.1, and 2.1, generated from the official JSON
  schemas, one module per version (`v16`, `v201`, `v21`).
- `ocpp-codegen`: a workspace-internal (unpublished) code generator driving all of the above from
  `schemas/`, plus `scripts/generate.sh` to regenerate and a CI job that fails if the committed
  output drifts from what the generator would actually produce.
- `no_std`, allocation-free by default: bounded fields use `heapless::String`/`heapless::Vec<T>`
  sized to the spec's stated `maxLength`/`maxItems`.
- Fields with no spec-given bound (a handful of free-text strings and arrays) expose a caller-
  chosen const generic parameter with a sensible default, instead of guessing a size.
- `alloc` feature: the same unbounded fields become plain `alloc::string::String`/
  `alloc::vec::Vec<T>` instead, for consumers with a real allocator.
- `serde` feature: `Serialize`/`Deserialize` on every type, with camelCase wire field names
  (`#[serde(rename = ...)]`) and optional fields omitted rather than serialized as `null`
  (`skip_serializing_if`).
- `Action` trait: every message exposes its OCPP action name as an associated constant, plus
  (with `serde`) zero-allocation JSON (de)serialization helpers backed by
  [`serde-json-core`](https://docs.rs/serde-json-core) (`to_json_str`/`to_json_slice`/
  `from_json_str`/`from_json_slice`).
- Per-version `RpcErrorCode` enums covering the `CALLERROR` codes from each version's OCPP-J
  specification, implementing `core::error::Error`. Not shared across versions: 2.0.1/2.1 renamed
  `FormationViolation` to `FormatViolation`, fixed a spelling error in
  `Occur(r)enceConstraintViolation`, and added `MessageTypeNotSupported`/`RpcFrameworkError`.
- `IdTag` (1.6J-specific wrapper primitive) and other version-specific hand-written primitives,
  alongside the generated types.
- Doc comments on generated types and fields, carried over from the schema's own `description`
  text.
- Cross-file deduplication: a type referenced by multiple messages (e.g. `CustomDataType`) is
  generated once into a shared `common.rs` per version instead of once per message file.
- Cargo publish metadata (`description`, `license`, `repository`, `keywords`, `categories`,
  `readme`) and a crate-level doc comment for the docs.rs landing page.
- CI: build/test across all feature combinations, plus the generated-types drift check.
- OCPP-J WebSocket envelopes (with `serde`): `Call<T>`/`CallResult<T>`/`CallError<E, D>` model the
  array-based `CALL`/`CALLRESULT`/`CALLERROR` wrapper every message travels in, generic over the
  payload/error-code type so one definition covers every version. `CallResultError`/`SendMessage`
  cover OCPP 2.1's additional `CALLRESULTERROR`/`SEND` message types. `Call`'s wire `Action` is
  validated against the payload type's `Action::ACTION` on deserialize rather than trusted.
  `MessageId` (max 36 chars, identical across all three versions) and `EmptyPayload` (for the
  spec-undefined `errorDetails`, default `{}`) round out the primitives.
- `crates/ocpp-types/examples/`: five runnable examples (basic usage, serialization, RPC error
  codes, unbounded-field capacities, WebSocket envelopes), each verified to actually run, not just
  compile.

[Unreleased]: https://github.com/flowionab/ocpp-types/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/flowionab/ocpp-types/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/flowionab/ocpp-types/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/flowionab/ocpp-types/releases/tag/v0.1.0
