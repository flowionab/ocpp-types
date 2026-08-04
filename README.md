# OCPP Types

[![Crates.io](https://img.shields.io/crates/v/ocpp-types.svg)](https://crates.io/crates/ocpp-types)
[![Documentation](https://docs.rs/ocpp-types/badge.svg)](https://docs.rs/ocpp-types)
[![License](https://img.shields.io/crates/l/ocpp-types)](#license)

A strongly typed Rust implementation of the Open Charge Point Protocol (OCPP) message model.

`ocpp-types` provides the request/response payload types for OCPP 1.6J, 2.0.1, and 2.1, generated
from the official JSON schemas. It's `no_std` and, by default, allocation-free: field sizes are
bounded at the type level with [`heapless`](https://docs.rs/heapless) collections, sized to the
limits stated in each version's specification.

The crate is designed to be lightweight and reusable across embedded firmware, Linux-based charge
points, simulators, CSMS implementations, and tooling.

---

## Supported Versions

| Version    | Status              |
| ---------- | ------------------- |
| OCPP 1.6J  | ✅ Available         |
| OCPP 2.0.1 | ✅ Available         |
| OCPP 2.1   | ✅ Available         |

Each version lives in its own module (`v16`, `v201`, `v21`), since the same message name can
differ in shape across versions.

---

## Example

```rust
use ocpp_types::Action;
use ocpp_types::v16::{BootNotificationRequest, IdTag};

let request = BootNotificationRequest {
    charge_point_vendor: "Flowion".try_into()?,
    charge_point_model: "Simulator".try_into()?,
    charge_box_serial_number: None,
    charge_point_serial_number: None,
    firmware_version: None,
    iccid: None,
    imsi: None,
    meter_serial_number: None,
    meter_type: None,
};

assert_eq!(BootNotificationRequest::ACTION, "BootNotification");
```

Every field that OCPP bounds with a `maxLength`/`maxItems` is sized exactly to that bound (a
`heapless::String<20>`, not an arbitrary `String`), so a value that doesn't fit fails at
construction (`try_into()`/`try_from()`), not somewhere downstream at serialization time.

### Runnable examples

[`crates/ocpp-types/examples/`](crates/ocpp-types/examples/) has complete, runnable programs for
the topics on this page:

```sh
cargo run -p ocpp-types --example basic_usage
cargo run -p ocpp-types --example serialization --features serde
cargo run -p ocpp-types --example rpc_error_codes
cargo run -p ocpp-types --example unbounded_fields              # const-generic capacity
cargo run -p ocpp-types --example unbounded_fields --features alloc  # alloc collections instead
cargo run -p ocpp-types --example envelope --features serde
```

---

## Serialization

With the `serde` feature enabled, every message implements `Serialize`/`Deserialize`, and the
[`Action`] trait gains zero-allocation JSON helpers backed by
[`serde-json-core`](https://docs.rs/serde-json-core) — the caller owns the buffer, nothing is
heap-allocated:

```rust
use ocpp_types::Action;

let mut buf = [0u8; 256];
let json: &str = request.to_json_str(&mut buf)?;
let parsed = BootNotificationRequest::from_json_str(json)?;
```

`to_json_slice`/`from_json_slice` are also available for working with raw bytes instead of `&str`.

---

## Fields with no spec-given bound

A handful of fields (free-text strings, a few arrays) have no `maxLength`/`maxItems` in the spec,
so there's no size to give a `heapless` collection without guessing one. These expose a const
generic parameter the caller can pick, defaulting to a reasonable size (1024 for strings, 16 for
arrays) so most code never has to think about it:

```rust
use ocpp_types::v16::HeartbeatResponse;

// Uses the default capacity:
let response: HeartbeatResponse = HeartbeatResponse {
    current_time: heapless::String::try_from("2024-01-01T00:00:00Z")?,
};

// Or pick a smaller one explicitly:
let response: HeartbeatResponse<64> = HeartbeatResponse {
    current_time: heapless::String::try_from("2024-01-01T00:00:00Z")?,
};
```

With the `alloc` feature enabled instead, these fields become plain `alloc::string::String` /
`alloc::vec::Vec<T>`, and the const generic disappears entirely — useful on targets with a real
allocator (a CSMS backend, a simulator) that would rather not pick a bound at all.

---

## RPC errors

Each version also exposes an `RpcErrorCode` enum covering the `CALLERROR` codes defined by that
version's OCPP-J specification, implementing `core::error::Error`. These aren't identical across
versions — 2.0.1/2.1 renamed `FormationViolation` to `FormatViolation`, fixed a spelling error in
`Occur(r)enceConstraintViolation`, and added two new codes — so each version's is its own type,
not shared:

```rust
use ocpp_types::v16::RpcErrorCode;

let error = RpcErrorCode::NotImplemented;
println!("{error}"); // "Requested Action is not known by receiver"
```

---

## WebSocket envelopes

With the `serde` feature, `Call`/`CallResult`/`CallError` model the OCPP-J array-based envelope
every message travels in — generic over the payload type, so there's one definition covering every
version rather than one per version:

```rust
use ocpp_types::v16::{AuthorizeRequest, IdTag};
use ocpp_types::{Call, MessageId};

let call = Call {
    message_id: MessageId::try_from("19223201")?,
    payload: AuthorizeRequest {
        id_tag: IdTag::try_from("ABC123")?,
    },
};

let mut buf = [0u8; 256];
let len = serde_json_core::to_slice(&call, &mut buf)?;
// [2,"19223201","Authorize",{"idTag":"ABC123"}]
```

The wire's `"Action"` string comes from `AuthorizeRequest::ACTION`, not a redundant stored field —
and is validated against it when parsing a `Call<T>` back, so a `Call<AuthorizeRequest>` you get out
really is one. `CallResultError`/`SendMessage` cover OCPP 2.1's additional `CALLRESULTERROR`/`SEND`
message types. See [`crates/ocpp-types/examples/envelope.rs`](crates/ocpp-types/examples/envelope.rs).

---

## Feature flags

| Feature | Default | Effect                                                                             |
| ------- | ------- | ----------------------------------------------------------------------------------- |
| `serde` | off     | `Serialize`/`Deserialize` on every type, plus [`Action`]'s JSON helpers.             |
| `alloc` | off     | Fields with no spec-given bound become `alloc` collections instead of const-generic `heapless` ones. Combine with `serde` to also serialize them. |

Without any features, the crate has exactly one dependency: [`heapless`](https://docs.rs/heapless).

---

## What this crate does

- OCPP request/response types for 1.6J, 2.0.1, and 2.1
- Common/shared data structures and enums, deduplicated per version
- `serde` serialization, including OCPP-J `CALLERROR` error codes and the `CALL`/`CALLRESULT`/
  `CALLERROR` (and 2.1's `CALLRESULTERROR`/`SEND`) WebSocket envelopes
- Doc comments carried over from the spec's own field/message descriptions

## What this crate does NOT do

This crate intentionally does **not** implement:

- The actual WebSocket transport (opening/maintaining the connection, framing, reconnects)
- Message routing, charge point state machines, or CSMS logic
- Smart charging algorithms

Those responsibilities belong in higher-level crates built on top of this one.

---

## Ecosystem

`ocpp-types` is intended as the foundation for a broader Rust OCPP ecosystem — the shared data
model that other crates build transport, state machines, and application logic on top of.

```text
                     ocpp-types
                          │
              ┌───────────┴───────────┐
              │                       │
        ocpp-transport         ocpp-charge-point
              │                       │
              └───────────┬───────────┘
                          │
                   Applications
```

| Crate                     | Purpose                                     | Status     |
| ------------------------- | -------------------------------------------- | ---------- |
| `ocpp-types`               | Protocol data model                         | ✅ This crate |
| `ocpp-transport`           | Message framing and transport abstractions  | 📋 Planned |
| `ocpp-charge-point`        | Charge Point implementation                 | 📋 Planned |
| `charge-point-simulator`   | OCPP testing and simulation tools           | 📋 Planned |

### Related projects

- **ocpp-charge-point** — a reusable Rust implementation of an OCPP Charge Point.
- **rust-ocpp** — shared Rust libraries for the Open Charge Point Protocol.

---

## How the types are generated

Nothing in `ocpp-types` is hand-written except a handful of primitives (`IdTag`, `RpcErrorCode`)
that have no equivalent in the JSON schemas. Everything else is generated by `ocpp-codegen` (a
workspace-internal, unpublished dev tool) from the schemas in `schemas/`. If a type looks wrong,
the fix belongs in the generator, not in a hand-edit of the generated file — every generated file
says as much at the top, and regenerating overwrites hand-edits anyway.

```sh
./scripts/generate.sh
```

CI regenerates and diffs on every push, so the committed output can't silently drift from what the
schemas and generator would actually produce.

---

## Design principles

- **Transport agnostic.** No knowledge of WebSockets or networking — this crate just models the
  protocol's data shapes.
- **`no_std` by default.** Works on embedded microcontrollers, Linux-based charge points, cloud
  services, and desktop simulators alike; `alloc` is opt-in, never required.
- **Specification-first.** Field names, bounds, and doc comments come directly from the OCPP JSON
  schemas and OCPP-J specifications, not reinterpreted by hand.

---

## License

Licensed under either of

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

at your option.

---

## Contributing

Contributions are welcome — implementing new message types, fixing specification inconsistencies,
adding tests, or improving documentation. Since types are generated, changes usually belong in
`ocpp-codegen` or `schemas/` rather than in `crates/ocpp-types/src` directly. Feel free to open an
issue or a pull request.
