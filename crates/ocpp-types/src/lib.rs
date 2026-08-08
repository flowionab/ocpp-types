//! Strongly typed OCPP message types for 1.6J, 2.0.1, and 2.1.
//!
//! `ocpp-types` provides the request/response payload types for the Open
//! Charge Point Protocol, generated from the official JSON schemas. It is
//! `no_std` and allocation-free: field sizes are bounded at the type level
//! with [`heapless`] collections, sized to the limits stated in each
//! version's specification.
//!
//! Each protocol version lives in its own module -- [`v16`], [`v201`],
//! [`v21`] -- since the same message name can differ in shape across
//! versions.
//!
//! # Fields with no spec-given bound
//!
//! A handful of fields (free-text strings, a few arrays) have no
//! `maxLength`/`maxItems` in the spec, so there's no size to give a
//! `heapless` collection without guessing one. These become a const
//! generic the caller picks (with a default, so most code never needs to
//! think about it):
//!
//! ```
//! # #[cfg(not(feature = "alloc"))]
//! # {
//! use ocpp_types::v16::DataTransferResponse;
//! use ocpp_types::v16::common::DataTransferResponseStatus;
//!
//! // Uses the default capacity (1024):
//! let response: DataTransferResponse = DataTransferResponse {
//!     data: Some(heapless::String::try_from("vendor payload").unwrap()),
//!     status: DataTransferResponseStatus::Accepted,
//! };
//!
//! // Or pick a smaller one explicitly:
//! let response: DataTransferResponse<64> = DataTransferResponse {
//!     data: Some(heapless::String::try_from("vendor payload").unwrap()),
//!     status: DataTransferResponseStatus::Accepted,
//! };
//! # }
//! ```
//!
//! With the `alloc` feature enabled, these fields become plain
//! `alloc::string::String`/`alloc::vec::Vec<T>` instead, and the const
//! generic disappears entirely -- useful on targets with a real allocator
//! (a CSMS backend, a simulator) that would rather not pick a bound at all.
//!
//! # Timestamps
//!
//! Every version types its `dateTime` fields as `{"type": "string",
//! "format": "date-time"}` without a `maxLength`, so they would fall under
//! the rule above and reserve 1024 bytes each. They are [`OcppTimestamp`]
//! instead: 16 bytes, no const generic, no allocator, and comparable --
//! which a string is not.
//!
//! ```
//! use ocpp_types::{OcppTimestamp, v16::HeartbeatResponse};
//!
//! let response = HeartbeatResponse {
//!     current_time: OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
//! };
//!
//! assert_eq!(response.current_time.unix_seconds(), 1_704_067_200);
//! ```
//!
//! Enable the `chrono` feature for `From`/`Into` conversions with
//! `chrono::DateTime`. That is interop only -- chrono is never on the wire
//! path, since its own serde support formats through an allocating
//! `to_rfc3339`, which the no-`alloc` build cannot use.
//!
//! # Sizing for allocation-free targets
//!
//! With `default-features = false` every field is stored inline at its
//! declared capacity, so a message's `size_of` is the sum of what it *could*
//! hold, not what it does. Most of the protocol is small under that rule --
//! the median message is a few hundred bytes and around 85% are under 4 KB --
//! but a few families reserve far more, and those need their capacities
//! named rather than defaulted.
//!
//! Every capacity below is a const generic with a default, so nothing has to
//! be specified to compile; specifying them is how a station trades unused
//! headroom for stack space.
//!
//! | If your station does | Set | Because the default is |
//! | --- | --- | --- |
//! | Smart charging | `chargingSchedulePeriod`, `chargingProfile` caps | 8 each, and they nest three deep |
//! | V2X / bidirectional | `v2xFreqWattCurve`, `v2xSignalWattCurve` | 8 points each, on every schedule period |
//! | ISO 15118-20 pricing | `priceRuleStacks`, `priceLevelScheduleEntries`, `salesTariffEntry` | 8 each; set to 0 if unused |
//! | Tariffs (2.1) | `energyPrices`, `timePrices`, `fixedPrices` | 8 each, per tariff kind |
//! | Plug and Charge | `certificate`, `certificateChain`, `csr`, `signingCertificate` | 1024 -- *too small for a real PEM chain* |
//! | Signed metering | `signedMeterData` | 1024; the spec allows 32768 |
//! | Local auth lists | `localAuthorizationList` | 8 entries |
//! | Metering | `meterValue`, `sampledValue` | 8 each, and they multiply |
//!
//! Two of these are worth calling out for opposite reasons. The Plug and
//! Charge fields are the only ones whose default is deliberately *too small*:
//! a PEM chain will not fit in 1024 bytes, so a deployment that uses
//! certificates must raise them and will discover this immediately in
//! testing. Erring the other way would have cost every deployment that never
//! sees a certificate. Conversely, capacities you set to `0` cost nothing at
//! all, which is the cheapest way to exclude a feature you do not implement.
//!
//! As a worked example, `ReportChargingProfilesRequest` defaults to 530 KB.
//! A station that advertises `PeriodsPerSchedule = 8`, reports one profile at
//! a time, and implements neither V2X nor ISO 15118-20 pricing compiles the
//! same message at 18 KB by naming those capacities.
//!
//! The specification expects this: `SmartChargingCtrlr.PeriodsPerSchedule` is
//! a required 2.x variable and 1.6 has `ChargingScheduleMaxPeriods`, so every
//! station already declares its own limits. Compiling in the capacity you
//! advertise is conformant; reserving the protocol ceiling you will never
//! accept is merely large.
//!
//! # Checking a payload against the spec
//!
//! Most spec limits are in the types, so a violation cannot be built: a
//! property bounded at `maxLength: 20` is a `heapless::String<20>`. What is
//! left over is what the `validate` feature covers — the bounds too large
//! to store inline (see the sizing table above: those fields are a growable
//! `String`/`Vec` under `alloc`, and a caller-chosen capacity without it),
//! plus every `minItems`, `minimum`, `maximum` and `multipleOf` in the
//! schemas, which no collection type can express at all.
//!
//! ```
//! # #[cfg(all(feature = "validate", feature = "alloc"))] {
//! use ocpp_types::v21::CancelReservationRequest;
//! use ocpp_types::validate::{Validate, ValidationErrorKind};
//!
//! let request: CancelReservationRequest = CancelReservationRequest {
//!     custom_data: None,
//!     reservation_id: -1, // the schema states `minimum: 0`
//! };
//!
//! let error = request.validate().unwrap_err();
//! assert_eq!(
//!     error.kind(),
//!     ValidationErrorKind::BelowMinimum { value: -1.0, min: 0.0 },
//! );
//! # }
//! ```
//!
//! This matters most on the sending side, and most of all for a CSMS: an
//! over-long field that the type accepted comes back from the peer as a
//! `CALLERROR` with no indication of which field caused it, while
//! [`validate::ValidationError`] names the path to it. Nothing calls it for
//! you — see the [`validate`] module.
//!
//! # Fields the spec leaves untyped
//!
//! 2.0.1 and 2.1's `DataTransfer` carries a `data` field the specification
//! deliberately gives no type at all -- "open to implementation", agreed
//! between the two parties. There's no single Rust type for arbitrary JSON
//! without an allocator, so the payload type is the caller's to pick, as a
//! type parameter defaulting to `()` (i.e. "this deployment sends no
//! data"):
//!
//! ```
//! use ocpp_types::v201::DataTransferRequest;
//!
//! // Whatever this vendor agreed on; add `serde::Serialize`/`Deserialize`
//! // to send it on the wire.
//! #[derive(Debug, Clone, PartialEq)]
//! struct VendorPayload {
//!     session_id: u32,
//! }
//!
//! let request: DataTransferRequest<VendorPayload> = DataTransferRequest {
//!     custom_data: None,
//!     data: Some(VendorPayload { session_id: 42 }),
//!     message_id: None,
//!     vendor_id: heapless::String::try_from("com.example").unwrap(),
//! };
//!
//! // Or, sending no vendor payload at all, the default:
//! let plain: DataTransferRequest = DataTransferRequest {
//!     custom_data: None,
//!     data: None,
//!     message_id: None,
//!     vendor_id: heapless::String::try_from("com.example").unwrap(),
//! };
//! ```
//!
//! 1.6J's `DataTransfer.data` is a plain string in that version's schema,
//! so it stays `Option<heapless::String<N>>` and needs no parameter.
//!
//! # Example
//!
//! ```
//! use ocpp_types::Action;
//! use ocpp_types::v16::{AuthorizeRequest, IdTag};
//!
//! let request = AuthorizeRequest {
//!     id_tag: IdTag::try_from("ABC123").unwrap(),
//! };
//!
//! assert_eq!(AuthorizeRequest::ACTION, "Authorize");
//! ```
//!
//! # Serialization
//!
//! With the `serde` feature enabled, every message implements
//! `serde::Serialize`/`serde::Deserialize`, and [`Action`] gains
//! zero-allocation JSON helpers backed by
//! [`serde-json-core`](https://docs.rs/serde-json-core) -- the caller owns
//! the buffer, nothing is heap-allocated:
//!
//! ```ignore
//! use ocpp_types::Action;
//!
//! let mut buf = [0u8; 256];
//! let json: &str = request.to_json_str(&mut buf)?;
//! let parsed = AuthorizeRequest::from_json_str(json)?;
//! ```
//!
//! # RPC errors
//!
//! Each version also exposes an `RpcErrorCode` enum covering the
//! `CALLERROR` codes defined by that version's OCPP-J specification (e.g.
//! [`v16::RpcErrorCode`]), implementing [`core::error::Error`].
//!
//! # WebSocket envelopes
//!
//! With `serde`, `Call`/`CallResult`/`CallError` model the OCPP-J
//! array-based envelope every message travels in (`[2, messageId, action,
//! payload]`, etc.) -- generic over the payload type, so no per-version
//! duplication is needed. `CallResultError`/`SendMessage` cover 2.1's
//! additional `CALLRESULTERROR`/`SEND` message types (the shapes work for
//! any version; whether a given deployment actually uses them is a
//! protocol-level concern, not something the types enforce). See
//! `examples/envelope.rs`.
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

mod action;
mod custom_data;
mod envelope;
mod timestamp;
// `ValidationError` carries the JSON path to the failing value inline --
// there is no allocator to box it into, and a path that names the field is
// the whole point of the error. That makes it larger than clippy's
// threshold, deliberately; see `validate::MAX_PATH_DEPTH`, and the size
// test that pins it.
#[cfg_attr(feature = "validate", allow(clippy::result_large_err))]
#[cfg(feature = "validate")]
pub mod validate;

#[cfg(test)]
mod generics_test;

#[cfg(test)]
mod size_test;

#[cfg(test)]
mod standard_test;

#[cfg(test)]
mod untyped_data_test;

#[cfg(all(test, feature = "validate"))]
mod validate_test;

#[cfg(test)]
mod v16_security_test;

pub mod v16;
pub mod v201;
pub mod v21;

pub use action::Action;
#[cfg(feature = "serde")]
pub use envelope::{Call, CallError, CallResult, CallResultError, EmptyPayload, SendMessage};
pub use custom_data::NoCustomData;
pub use envelope::MessageId;
pub use timestamp::{MAX_RFC3339_LEN, OcppDate, OcppTimeOfDay, OcppTimestamp, TimestampError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_tag_round_trips_a_short_string() {
        let tag = v16::IdTag::try_from("ABC123").unwrap();
        assert_eq!(tag.as_str(), "ABC123");
    }

    #[test]
    fn id_tag_rejects_strings_over_20_bytes() {
        let too_long = "A".repeat(21);
        assert!(v16::IdTag::try_from(too_long.as_str()).is_err());
    }

    /// See `envelope::tests::message_id_error_type_stays_unit_...`: the
    /// wrapper's `Error` is this crate's public API and must not track
    /// whatever `heapless::String::try_from` happens to return.
    #[test]
    fn id_tag_error_type_stays_unit_regardless_of_the_heapless_version() {
        fn assert_unit_error<T>()
        where
            for<'a> T: TryFrom<&'a str, Error = ()>,
        {
        }

        assert_unit_error::<v16::IdTag>();
    }

    struct DummyRequest;

    impl Action for DummyRequest {
        const ACTION: &'static str = "Dummy";
    }

    #[test]
    fn action_trait_exposes_action_name() {
        assert_eq!(DummyRequest::ACTION, "Dummy");
    }
}
