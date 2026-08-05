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
//! use ocpp_types::v16::HeartbeatResponse;
//!
//! // Uses the default capacity (1024):
//! let response: HeartbeatResponse = HeartbeatResponse {
//!     current_time: heapless::String::try_from("2024-01-01T00:00:00Z").unwrap(),
//! };
//!
//! // Or pick a smaller one explicitly:
//! let response: HeartbeatResponse<64> = HeartbeatResponse {
//!     current_time: heapless::String::try_from("2024-01-01T00:00:00Z").unwrap(),
//! };
//! # }
//! ```
//!
//! With the `alloc` feature enabled, these fields become plain
//! `alloc::string::String`/`alloc::vec::Vec<T>` instead, and the const
//! generic disappears entirely -- useful on targets with a real allocator
//! (a CSMS backend, a simulator) that would rather not pick a bound at all.
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
mod envelope;

#[cfg(test)]
mod generics_test;

#[cfg(test)]
mod untyped_data_test;

pub mod v16;
pub mod v201;
pub mod v21;

pub use action::Action;
#[cfg(feature = "serde")]
pub use envelope::{Call, CallError, CallResult, CallResultError, EmptyPayload, SendMessage};
pub use envelope::MessageId;

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
