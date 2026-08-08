# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-08-08

### Added

- 1.6J: the 11 Security Whitepaper message pairs, absent until now — `CertificateSigned`,
  `DeleteCertificate`, `ExtendedTriggerMessage`, `GetInstalledCertificateIds`, `GetLog`,
  `InstallCertificate`, `LogStatusNotification`, `SecurityEventNotification`, `SignCertificate`,
  `SignedFirmwareStatusNotification` and `SignedUpdateFirmware`.

  The OCA publishes no JSON schemas for these messages (they exist only as prose in the Security
  Whitepaper), so the 22 schema files were authored: 8 pairs adapted from the 2.0.1 schemas already
  vendored here, 3 pairs (`ExtendedTriggerMessage`, `SignedUpdateFirmware`,
  `SignedFirmwareStatusNotification`) from the whitepaper alone, since 2.0.1 has no counterpart.
  Provenance and the bounds most worth verifying are documented in `schemas/ocpp1.6j/README.md`.

- 1.6J: a `standard` module with the 50 standard configuration keys (Core, Local Auth List
  Management, Reservation, Smart Charging, plus the Security Whitepaper's) and the 20 security event
  types. Transcribed from spec prose rather than exported — see `csv/ocpp1.6j/README.md`.

- 2.0.1/2.1: a `standard` module per version holding the value sets the specification defines for
  fields its JSON schemas type as bare strings — `Component.name`, `Variable.name`,
  `SecurityEventNotification.type`, `StatusInfo.reasonCode`, `UnitOfMeasure.unit`, and for 2.1 also
  `connectorType`, `IdToken.type`, `chargingLimitSource`, `paymentBrand`, `paymentRecognition`,
  `signingMethod` and `AdditionalInfo.type`. 517 named values for 2.0.1 and 629 for 2.1, plus a
  `DEVICE_MODEL` table of the 438 standardized `(component, variable)` pairs.

  These are purely additive: OCPP permits vendor-specific components, variables and security
  events, so narrowing a wire field from a string to an enum would make the crate reject conformant
  traffic. The message structs keep their `heapless::String` fields; each enum sits alongside with
  `as_str`, `from_wire` (`None` for a value the spec doesn't define, which is not an error),
  `FromStr`, `Display`, an `ALL` slice, and accessors for the metadata the spec states — e.g.
  `SecurityEvent::is_critical`, `VariableName::data_type`, `SigningMethod::curve`.

  Exposed as `v201::standard::*` / `v21::standard::*` rather than re-exported into the version
  module, since several value sets share a name with a schema-generated type (`UnitOfMeasure`,
  `Component`, `IdToken`).

- `ocpp-codegen` gained a `--csv <dir>` input for these tables, which the schemas cannot supply.
  Source data lives in `csv/<version>/`.

- The value sets a deployment realistically extends — `v16::standard::ConfigurationKey`, and
  `ComponentName`/`VariableName` for 2.0.1 and 2.1 — are *open*: they carry an
  `Other(heapless::String<50>)` variant, so a third-party value deserializes and serializes back
  unchanged instead of failing. `from_wire_or_other` accepts anything the field can hold (50 bytes,
  the schema's own `maxLength`); `from_wire` still answers the narrower "is this a spec value?" with
  `Option`, and `is_standardized` distinguishes the two after the fact.

  Equality, ordering and hashing on these compare the wire string rather than the variant, so
  `Other("HeartbeatInterval")` equals `ConfigurationKey::HeartbeatInterval` — deriving them would
  have made equality depend on how a value was constructed, which is a quiet way to get a lookup
  miss. Prefer `from_wire_or_other` over building `Other` directly and the question doesn't arise.

  The cost, and the reason this isn't done for every value set: an open enum is not `Copy` and grows
  from 1 byte to 72, and its serde impls are hand-written since serde cannot express "unit variants
  plus a string fallback". The remaining sets (`SecurityEvent`, `ReasonCode`, `Unit`,
  `ConnectorType`, ...) stay closed and 1 byte. Opening another is a one-line change in
  `crates/ocpp-codegen/src/standard.rs`.


- Size budgets for the message types (`crates/ocpp-types/src/size_test.rs`), covering the hot-path
  messages, the charging-profile family, and the leaf types that drive both, in each feature
  configuration.

  Two tests, doing different jobs. A **ratchet** asserts every type against its currently-measured
  size and runs in CI — it passes today, so a schema or generator change that inflates a type now
  fails immediately instead of going unnoticed. A **target** asserts a 4 KB per-message MCU budget;
  it is `#[ignore]`d because it fails today, by up to 313,085× (`v21::ReportChargingProfilesRequest`
  is 1,282,397,392 bytes without `alloc`). Run it with
  `cargo test -p ocpp-types -- --ignored size_test`.

  Recording the sizes surfaced two problems beyond the reported `ChargingProfile` one: **1.6J is
  affected too** — `v16::MeterValuesRequest` is 282,912 bytes without `alloc`, from three nested
  unbounded arrays — and **`AuthorizeRequest`, a hot-path message, is 16,776 bytes (2.0.1) and
  30,920 bytes (2.1)**, driven by `IdToken.additionalInfo` rather than by charging profiles, so it
  needs fixing independently of them.

  No sizes changed in this release; the tests only record and guard them.


- `OcppTimestamp`, a 16-byte fixed-size type for every version's `dateTime` fields, replacing the
  `heapless::String`/`alloc::String` they used to be.

  All three versions type timestamps identically (`{"type": "string", "format": "date-time"}`) and
  **none of the 142 such fields declares a `maxLength`**, so each one took the generator's 1024-byte
  unbounded default. `v16::HeartbeatResponse` is now **16 bytes, down from 1,032** — and identical in
  both feature configurations, since the const generic and the `alloc`/no-`alloc` split both
  disappear with it.

  Parses RFC 3339 as the specification requires (`T`/`t`/space separators, `Z`/`±HH:MM` offsets,
  fractional seconds to nanosecond precision) and writes a canonical form. The written UTC offset is
  preserved rather than normalized, so a peer's local-time timestamp round-trips as sent. Equality
  and ordering are by instant, not by written form: `2024-01-01T00:00:00Z` equals
  `2024-01-01T01:00:00+01:00`, because they are the same moment.

- **Breaking, 2.0.1/2.1:** `customData` is now a caller-chosen type parameter, defaulting to
  `NoCustomData` — a zero-sized stand-in that makes `Option<NoCustomData>` one byte, against 272 for
  the specification's shape.

  2.x hangs `customData` on nearly every object (151 structs in 2.1), and by-value nesting multiplies
  that 272 bytes by the whole type graph rather than paying it once. `v21::ChargingProfile` drops
  from 522,328 to **69,224** bytes without `alloc`; `v201::ChargingProfile` from 111,000 to
  **12,352**; `v21::ChargingSchedulePeriod` from 5,168 to **672**.

  `NoCustomData` is permissive, not empty: it accepts and discards any `customData` object a peer
  sends, because `()` would accept only `null` and would fail to parse conformant traffic. Discarded
  data is not echoed back. Callers who need the contents name a type instead —
  `Component<CustomData>` for the specification's own shape (still generated), or their own struct
  for a vendor extension.

  One shared parameter is threaded across each version rather than one per field, so a struct
  containing ten others that each carry `customData` declares one parameter, not ten. It is ordered
  *last*, so existing positional arguments such as `DataTransferRequest<VendorPayload>` still bind
  as before.

- A `chrono` feature adding `From`/`Into` between `OcppTimestamp` and `chrono::DateTime<Utc>` /
  `DateTime<FixedOffset>`.

  Interop only, deliberately not the representation: chrono's own serde support formats through an
  allocating `to_rfc3339`, which the no-`alloc` build — the one these sizes matter most for — cannot
  use. Keeping chrono at the boundary gives its ergonomics to callers who want them without putting
  an allocator on anyone else's wire path. The feature is additive; no message type changes shape.

### Changed

- **Breaking:** `alloc` is now a **default feature**. Without it every field is stored inline at its
  declared capacity — the point of the crate on an MCU, but a poor default for the majority of users
  (a CSMS, a simulator, a test harness) who have an allocator and would otherwise meet
  multi-kilobyte message types for no reason. The allocation-free build is now
  `default-features = false`, and the crate docs gained a per-family sizing table for it.


- **Breaking, 1.6J:** `v16::RequestedMessage` is now `v16::TriggerMessageRequestRequestedMessage`.
  Adding `ExtendedTriggerMessage` introduced a second `requestedMessage` property with a different
  set of values (it adds `SignChargePointCertificate` and `LogStatusNotification`, and drops
  `DiagnosticsStatusNotification`), so the generator's cross-file collision handling owner-qualified
  both names. The two enums have to stay distinct: sharing one would let a caller ask
  `TriggerMessage` to trigger something only `ExtendedTriggerMessage` can express. Callers using
  `TriggerMessageRequest` need to rename the type; the variants and wire values are unchanged.


- `ocpp-codegen` (dev-time only, not part of the published crate): updated to `prettyplease` 0.3 /
  `syn` 3. Generated output is byte-identical, so no `ocpp-types` source changed.
- `heapless` deliberately stays at 0.8. `serde-json-core` 0.6 — the latest — still depends on
  heapless 0.8, so taking 0.9 would link *two* copies of heapless into any downstream binary using
  the `serde` feature. The `IdTag`/`MessageId` `TryFrom<&str>` impls no longer let heapless's error
  type surface in this crate's public API (0.9 returns `CapacityError` where 0.8 returned `()`), so
  the bump is a one-line change once `serde-json-core` follows.

### Fixed

- 2.1's civil time and date fields are now `OcppTimeOfDay` (2 bytes) and `OcppDate` (4 bytes)
  instead of strings, matching how `dateTime` became `OcppTimestamp`. 2.1's tariff conditions were the case: `startTimeOfDay`/`endTimeOfDay` are
  `HH:MM` and `validFromDate`/`validToDate` are `YYYY-MM-DD` — the property descriptions give the
  regex — but none declares a `maxLength`. Four of them sit in `TariffConditions`, multiplied through
  three tariff kinds and their price arrays.

  `startTimeOfDay`/`endTimeOfDay` are `HH:MM` and `validFromDate`/`validToDate` are `YYYY-MM-DD` —
  the property descriptions give the regex — but none declares a `maxLength`, so each took the
  1024-byte unbounded default. Four sit in `TariffConditions`, multiplied through three tariff kinds
  and their price arrays.

  Without `alloc`: `TariffConditions` 4,384 → **240** bytes, `v21::AuthorizeResponse` 227,328 →
  **28,416**, `v21::ChangeTransactionTariffRequest` 223,104 → **24,192**.

  The larger gain is ergonomic: the const parameters go with the strings. `AuthorizeResponse` drops
  from 15 capacity parameters to **7**, and the two tariff requests from 12 to **4** — two-thirds of
  their parameter burden was four fields that can only ever hold a time or a date. The types also
  validate (`"25:99"` and `2023-02-29` are rejected) and order chronologically, which a string
  cannot.

  Unlike `date-time`, these properties declare no `format`, so the generator matches them by name.
  That is more fragile, so a test asserts the four names still exist as unbounded strings in the
  vendored 2.1 schemas — a rename would otherwise silently revert them to 1024-byte strings.

  Found by sweeping all 387 message types rather than the curated size-test list, which had been
  missing five of the eight largest. Those are now tracked too.

- Large spec-bounded **strings** are now a caller-chosen capacity too, mirroring the array rule. A
  string declaring more than 512 bytes becomes a const generic defaulting to 1024 (never above the
  spec's own ceiling); identifiers, hashes and short free text keep their exact bound and add no
  parameter.

  The specification states generous ceilings for fields that are usually short or absent, and the
  generator inlined every one of them: `signedMeterData` at 32,768, `ocspResult` at 18,000,
  `exiResponse` at 17,000, five `certificate`/`certificateChain` fields at 10,000, sixteen at 2,000.
  `signedMeterData` sits inside `SampledValue`, so it was multiplied by both the sampled-value and
  meter-value arrays — which is most of what made `TransactionEventRequest` megabytes, and is why
  neither the array nor the `customData` change had moved it.

  Without `alloc`: `v21::TransactionEventRequest` 2,281,808 → **155,472** (15×),
  `v21::AuthorizeRequest` 22,392 → **9,512**, `v201::AuthorizeRequest` 9,968 → **5,488**. With
  `alloc`, `v21::AuthorizeRequest` 19,712 → **1,784** (11×).

  The 1024 default is deliberately not enough for a PEM certificate chain: a deployment doing Plug
  and Charge must raise `certificate`, `certificateChain`, `csr`, `signingCertificate`,
  `exiRequest`/`exiResponse` and `signedMeterData` explicitly. Erring low costs those deployments a
  deserialize failure they hit immediately in testing; erring high would cost every deployment that
  never sees a certificate.

- Large spec-bounded arrays are now a caller-chosen capacity instead of being inlined at their
  `maxItems`. An array declaring more than 16 items becomes a const generic (defaulting to 16, never
  above the spec ceiling), the same mechanism unbounded arrays already used; smaller arrays are
  unchanged, so no extra parameter appears where it wouldn't pay for itself.

  This was the dominant term in the charging-profile sizes, and it was *not* the
  `DEFAULT_VEC_CAPACITY` fallback — `chargingSchedulePeriod` declares `maxItems: 1024` in the schema
  (seventeen 2.1 arrays do), and the generator inlined that ceiling. Reserving it is also more than
  OCPP asks of a station: `SmartChargingCtrlr.PeriodsPerSchedule` is a required 2.x variable and 1.6
  has `ChargingScheduleMaxPeriods`, so every station already declares its own lower limit.

  Without `alloc`: `v21::ChargingProfile` 80,149,816 → 1,221,688 bytes (66×),
  `v21::ReportChargingProfilesRequest` 1,282,397,392 → 19,547,344 (66×), `v201::ChargingProfile`
  13,893,040 → 224,560 (62×). With `alloc`, `v21::ChargingSchedulePeriod` 12,080 → 576 (21×), since
  the V2X curve arrays now heap-allocate rather than inlining 20 elements.

  Still short of the 4 KB budget — `CustomData` at 264 bytes is now the dominant remaining term, and
  is next.

- 2.0.1: `v201::standard` no longer contains 2.1-only names. Four of the supplied 2.0.1 spec tables
  were byte-identical 2.1 exports, which put components like `ACDERCtrlr`, `BatterySwapCtrlr` and
  `WebPaymentsCtrlr`, and variables like `MaxPeriodicEventStreams` and `SupportsEvseSleep`, into a
  version whose specification has no such features. Narrowed using the 2.1-only message list (from
  the schemas) and the per-version `variables.csv` difference; `csv/ocpp2.0.1/README.md` records what
  was removed, on what evidence, and what is still unverified.

- 2.0.1/2.1: corrected `TxCtlrl` to `TxCtrlr` in the device model table — one row against ten
  correctly-spelled siblings, which had been generating a `ComponentName::TxCtlrl` variant no
  Charging Station would send.

## [0.1.3]

### Fixed

- 2.0.1/2.1: `DataTransferRequest.data` and `DataTransferResponse.data` were generated as
  `Option<()>`, making the field structurally incapable of carrying the vendor payload the message
  exists to transport — the only workaround was to hand-write the wire struct and bypass
  `ocpp-types` for that message. The schemas give these properties no type at all (the spec leaves
  them "open to implementation"), and `ocpp-codegen` mapped an untyped property to the unit type.
  Such properties now become a caller-chosen generic type parameter defaulting to `()`, e.g.
  `DataTransferRequest<VendorPayload>` with `data: Option<VendorPayload>`. Existing code that never
  populated `data` is unaffected — the default keeps `DataTransferRequest` spelled exactly as
  before. 1.6J is unaffected: its schema types `data` as a string, so it was already
  `Option<heapless::String<N>>`.

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

[Unreleased]: https://github.com/flowionab/ocpp-types/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/flowionab/ocpp-types/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/flowionab/ocpp-types/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/flowionab/ocpp-types/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/flowionab/ocpp-types/releases/tag/v0.1.0
