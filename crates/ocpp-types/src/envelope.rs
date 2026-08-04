//! OCPP-J WebSocket message envelopes (`CALL`/`CALLRESULT`/`CALLERROR`, and
//! 2.1's `CALLRESULTERROR`/`SEND`) -- the array-based wrapper every OCPP
//! message travels in, as opposed to the message payload types themselves
//! (`v16`/`v201`/`v21`). See the OCPP-J specification, section 4.

/// A unique identifier correlating a `Call`/`SendMessage` with its
/// `CallResult`/`CallError`/`CallResultError`. Identical across 1.6J,
/// 2.0.1, and 2.1: a string of at most 36 characters, to allow for
/// UUIDs/GUIDs (per the OCPP-J specification).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MessageId(heapless::String<36>);

impl MessageId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for MessageId {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        heapless::String::try_from(value).map(Self)
    }
}

/// Placeholder for OCPP-J's `errorDetails`, which the specification
/// deliberately leaves undefined in shape ("this JSON object describes
/// error details in an undefined way") and recommends leaving empty in
/// practice. Serializes as `{}`; deserializes by accepting and discarding
/// whatever object is present, so it round-trips regardless of what a
/// real implementation happens to send. Use a custom type via
/// [`CallError`]/[`CallResultError`]'s `D` parameter for typed details.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EmptyPayload;

#[cfg(feature = "serde")]
impl serde::Serialize for EmptyPayload {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        serializer.serialize_map(Some(0))?.end()
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for EmptyPayload {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EmptyPayloadVisitor;

        impl<'de> serde::de::Visitor<'de> for EmptyPayloadVisitor {
            type Value = EmptyPayload;

            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("a JSON object")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                while map
                    .next_entry::<serde::de::IgnoredAny, serde::de::IgnoredAny>()?
                    .is_some()
                {}
                Ok(EmptyPayload)
            }
        }

        deserializer.deserialize_map(EmptyPayloadVisitor)
    }
}

/// An OCPP-J `CALL`: `[2, "<MessageId>", "<Action>", {Payload}]`. `T`'s
/// action name is written on serialize and validated (not merely assumed)
/// on deserialize, so a `Call<T>` you get back always really is the `T`
/// you asked for.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call<T> {
    pub message_id: MessageId,
    pub payload: T,
}

#[cfg(feature = "serde")]
impl<T: crate::Action + serde::Serialize> serde::Serialize for Call<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeTuple;

        let mut tup = serializer.serialize_tuple(4)?;
        tup.serialize_element(&2u8)?;
        tup.serialize_element(&self.message_id)?;
        tup.serialize_element(T::ACTION)?;
        tup.serialize_element(&self.payload)?;
        tup.end()
    }
}

#[cfg(feature = "serde")]
impl<'de, T: crate::Action + serde::Deserialize<'de>> serde::Deserialize<'de> for Call<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct CallVisitor<T>(core::marker::PhantomData<T>);

        impl<'de, T: crate::Action + serde::Deserialize<'de>> serde::de::Visitor<'de> for CallVisitor<T> {
            type Value = Call<T>;

            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "a CALL array [2, messageId, \"{}\", payload]", T::ACTION)
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let message_type_id: u8 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                if message_type_id != 2 {
                    return Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Unsigned(message_type_id as u64),
                        &"2 (CALL)",
                    ));
                }

                let message_id: MessageId = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;

                let action: &str = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;
                if action != T::ACTION {
                    return Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Str(action),
                        &T::ACTION,
                    ));
                }

                let payload: T = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(3, &self))?;

                Ok(Call {
                    message_id,
                    payload,
                })
            }
        }

        deserializer.deserialize_tuple(4, CallVisitor(core::marker::PhantomData))
    }
}

/// An OCPP-J `CALLRESULT`: `[3, "<MessageId>", {Payload}]`. Unlike
/// [`Call`], there's no action name on the wire -- the recipient already
/// knows which action a result is for by correlating `message_id` against
/// the `Call` it sent, which is exactly what choosing `T` here represents.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallResult<T> {
    pub message_id: MessageId,
    pub payload: T,
}

#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for CallResult<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeTuple;

        let mut tup = serializer.serialize_tuple(3)?;
        tup.serialize_element(&3u8)?;
        tup.serialize_element(&self.message_id)?;
        tup.serialize_element(&self.payload)?;
        tup.end()
    }
}

#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for CallResult<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct CallResultVisitor<T>(core::marker::PhantomData<T>);

        impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for CallResultVisitor<T> {
            type Value = CallResult<T>;

            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("a CALLRESULT array [3, messageId, payload]")
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let message_type_id: u8 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                if message_type_id != 3 {
                    return Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Unsigned(message_type_id as u64),
                        &"3 (CALLRESULT)",
                    ));
                }

                let message_id: MessageId = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;

                let payload: T = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;

                Ok(CallResult {
                    message_id,
                    payload,
                })
            }
        }

        deserializer.deserialize_tuple(3, CallResultVisitor(core::marker::PhantomData))
    }
}

/// Defines an error envelope type (`CallError`/`CallResultError`): both
/// are `[<discriminant>, "<MessageId>", "<errorCode>", "<errorDescription>",
/// {errorDetails}]`, differing only in which prior message they're a
/// response to and their `MessageTypeId`. A macro rather than two
/// hand-written copies, so a fix to one can't drift from the other.
macro_rules! error_envelope {
    ($name:ident, $discriminant:literal, $expecting:literal) => {
        #[cfg(feature = "serde")]
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name<E, D = EmptyPayload, const DESC_CAP: usize = 1024> {
            pub message_id: MessageId,
            pub error_code: E,
            pub error_description: heapless::String<DESC_CAP>,
            pub error_details: D,
        }

        #[cfg(feature = "serde")]
        impl<E: serde::Serialize, D: serde::Serialize, const DESC_CAP: usize> serde::Serialize
            for $name<E, D, DESC_CAP>
        {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeTuple;

                let mut tup = serializer.serialize_tuple(5)?;
                tup.serialize_element(&$discriminant)?;
                tup.serialize_element(&self.message_id)?;
                tup.serialize_element(&self.error_code)?;
                tup.serialize_element(&self.error_description)?;
                tup.serialize_element(&self.error_details)?;
                tup.end()
            }
        }

        #[cfg(feature = "serde")]
        impl<'de, E: serde::Deserialize<'de>, D: serde::Deserialize<'de>, const DESC_CAP: usize>
            serde::Deserialize<'de> for $name<E, D, DESC_CAP>
        {
            fn deserialize<Dz: serde::Deserializer<'de>>(deserializer: Dz) -> Result<Self, Dz::Error> {
                struct Visitor<E, D, const DESC_CAP: usize>(core::marker::PhantomData<(E, D)>);

                impl<'de, E: serde::Deserialize<'de>, D: serde::Deserialize<'de>, const DESC_CAP: usize>
                    serde::de::Visitor<'de> for Visitor<E, D, DESC_CAP>
                {
                    type Value = $name<E, D, DESC_CAP>;

                    fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                        f.write_str($expecting)
                    }

                    fn visit_seq<A: serde::de::SeqAccess<'de>>(
                        self,
                        mut seq: A,
                    ) -> Result<Self::Value, A::Error> {
                        let message_type_id: u8 = seq
                            .next_element()?
                            .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                        if message_type_id != $discriminant {
                            return Err(serde::de::Error::invalid_value(
                                serde::de::Unexpected::Unsigned(message_type_id as u64),
                                &$expecting,
                            ));
                        }

                        let message_id = seq
                            .next_element()?
                            .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                        let error_code = seq
                            .next_element()?
                            .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;
                        let error_description = seq
                            .next_element()?
                            .ok_or_else(|| serde::de::Error::invalid_length(3, &self))?;
                        let error_details = seq
                            .next_element()?
                            .ok_or_else(|| serde::de::Error::invalid_length(4, &self))?;

                        Ok($name {
                            message_id,
                            error_code,
                            error_description,
                            error_details,
                        })
                    }
                }

                deserializer.deserialize_tuple(5, Visitor(core::marker::PhantomData))
            }
        }
    };
}

// An OCPP-J `CALLERROR`: `[4, "<MessageId>", "<errorCode>",
// "<errorDescription>", {errorDetails}]`. `E` is the version's own
// `RpcErrorCode` (e.g. `v16::RpcErrorCode`) -- error codes aren't shared
// across versions, so nothing here picks one for you. `D` defaults to
// `EmptyPayload` since the spec leaves `errorDetails` undefined in shape
// and recommends leaving it empty; supply your own `D` for typed details.
// `errorDescription` has no spec-given bound either, so it's a
// caller-choosable const generic (default 1024) rather than a guess.
error_envelope!(
    CallError,
    4u8,
    "a CALLERROR array [4, messageId, errorCode, errorDescription, errorDetails]"
);

// An OCPP-J `CALLRESULTERROR` (OCPP 2.1 only): identical shape to
// `CallError`, sent in response to a `CallResult` that itself contained
// errors, rather than to a `Call`.
error_envelope!(
    CallResultError,
    5u8,
    "a CALLRESULTERROR array [5, messageId, errorCode, errorDescription, errorDetails]"
);

/// An OCPP-J `SEND` (OCPP 2.1 only): `[6, "<MessageId>", "<Action>",
/// {Payload}]`. Structurally identical to [`Call`], but the receiver
/// SHALL NOT respond to it -- used for fire-and-forget messages like
/// frequent periodic monitoring values.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendMessage<T> {
    pub message_id: MessageId,
    pub payload: T,
}

#[cfg(feature = "serde")]
impl<T: crate::Action + serde::Serialize> serde::Serialize for SendMessage<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeTuple;

        let mut tup = serializer.serialize_tuple(4)?;
        tup.serialize_element(&6u8)?;
        tup.serialize_element(&self.message_id)?;
        tup.serialize_element(T::ACTION)?;
        tup.serialize_element(&self.payload)?;
        tup.end()
    }
}

#[cfg(feature = "serde")]
impl<'de, T: crate::Action + serde::Deserialize<'de>> serde::Deserialize<'de> for SendMessage<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SendVisitor<T>(core::marker::PhantomData<T>);

        impl<'de, T: crate::Action + serde::Deserialize<'de>> serde::de::Visitor<'de> for SendVisitor<T> {
            type Value = SendMessage<T>;

            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "a SEND array [6, messageId, \"{}\", payload]", T::ACTION)
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let message_type_id: u8 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                if message_type_id != 6 {
                    return Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Unsigned(message_type_id as u64),
                        &"6 (SEND)",
                    ));
                }

                let message_id: MessageId = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;

                let action: &str = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;
                if action != T::ACTION {
                    return Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Str(action),
                        &T::ACTION,
                    ));
                }

                let payload: T = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(3, &self))?;

                Ok(SendMessage {
                    message_id,
                    payload,
                })
            }
        }

        deserializer.deserialize_tuple(4, SendVisitor(core::marker::PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The crate is `#![no_std]`, so `std` isn't implicitly in scope even in
    // test builds -- only needed here to spawn a thread with a larger
    // stack (see `send_message_serializes_and_round_trips_a_real_send_type_message`).
    extern crate std;

    #[cfg(feature = "serde")]
    #[test]
    fn empty_payload_serializes_as_an_empty_object() {
        let mut buf = [0u8; 16];
        let len = serde_json_core::to_slice(&EmptyPayload, &mut buf).unwrap();
        assert_eq!(&buf[..len], b"{}");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn empty_payload_accepts_and_discards_a_populated_object() {
        let (parsed, _): (EmptyPayload, usize) =
            serde_json_core::from_str(r#"{"foo": 1, "bar": [1, 2, 3]}"#).unwrap();
        assert_eq!(parsed, EmptyPayload);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn call_serializes_and_round_trips_a_real_message_type() {
        use crate::v16::{AuthorizeRequest, IdTag};

        let call = Call {
            message_id: MessageId::try_from("19223201").unwrap(),
            payload: AuthorizeRequest {
                id_tag: IdTag::try_from("ABC123").unwrap(),
            },
        };

        let mut buf = [0u8; 128];
        let len = serde_json_core::to_slice(&call, &mut buf).unwrap();
        assert_eq!(
            core::str::from_utf8(&buf[..len]).unwrap(),
            r#"[2,"19223201","Authorize",{"idTag":"ABC123"}]"#
        );

        let (parsed, _): (Call<AuthorizeRequest>, usize) =
            serde_json_core::from_slice(&buf[..len]).unwrap();
        assert_eq!(parsed, call);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn call_rejects_the_wrong_message_type_id() {
        use crate::v16::AuthorizeRequest;

        let json = r#"[3,"1","Authorize",{"idTag":"ABC123"}]"#;
        let result: Result<(Call<AuthorizeRequest>, usize), _> = serde_json_core::from_str(json);
        assert!(result.is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn call_rejects_a_mismatched_action() {
        use crate::v16::AuthorizeRequest;

        let json = r#"[2,"1","Heartbeat",{"idTag":"ABC123"}]"#;
        let result: Result<(Call<AuthorizeRequest>, usize), _> = serde_json_core::from_str(json);
        assert!(result.is_err());
    }

    // `HeartbeatResponse::current_time` is `heapless::String<N>` without
    // `alloc` and `alloc::string::String` with it -- this test constructs
    // the no-`alloc` shape specifically.
    #[cfg(all(feature = "serde", not(feature = "alloc")))]
    #[test]
    fn call_result_serializes_and_round_trips_a_real_message_type() {
        use crate::v16::HeartbeatResponse;

        let result: CallResult<HeartbeatResponse> = CallResult {
            message_id: MessageId::try_from("19223201").unwrap(),
            payload: HeartbeatResponse {
                current_time: heapless::String::try_from("2013-02-01T20:53:32.486Z").unwrap(),
            },
        };

        let mut buf = [0u8; 128];
        let len = serde_json_core::to_slice(&result, &mut buf).unwrap();
        assert_eq!(
            core::str::from_utf8(&buf[..len]).unwrap(),
            r#"[3,"19223201",{"currentTime":"2013-02-01T20:53:32.486Z"}]"#
        );

        let (parsed, _): (CallResult<HeartbeatResponse>, usize) =
            serde_json_core::from_slice(&buf[..len]).unwrap();
        assert_eq!(parsed, result);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn call_result_rejects_the_wrong_message_type_id() {
        use crate::v16::HeartbeatResponse;

        let json = r#"[2,"1",{"currentTime":"2013-02-01T20:53:32.486Z"}]"#;
        let result: Result<(CallResult<HeartbeatResponse>, usize), _> =
            serde_json_core::from_str(json);
        assert!(result.is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn call_error_serializes_and_round_trips_a_real_error_code() {
        use crate::v16::RpcErrorCode;

        let error: CallError<RpcErrorCode> = CallError {
            message_id: MessageId::try_from("19223201").unwrap(),
            error_code: RpcErrorCode::NotImplemented,
            error_description: heapless::String::try_from("unrecognized action").unwrap(),
            error_details: EmptyPayload,
        };

        let mut buf = [0u8; 128];
        let len = serde_json_core::to_slice(&error, &mut buf).unwrap();
        assert_eq!(
            core::str::from_utf8(&buf[..len]).unwrap(),
            r#"[4,"19223201","NotImplemented","unrecognized action",{}]"#
        );

        let (parsed, _): (CallError<RpcErrorCode>, usize) =
            serde_json_core::from_slice(&buf[..len]).unwrap();
        assert_eq!(parsed, error);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn call_error_rejects_the_wrong_message_type_id() {
        use crate::v16::RpcErrorCode;

        let json = r#"[5,"1","NotImplemented","",{}]"#;
        let result: Result<(CallError<RpcErrorCode>, usize), _> = serde_json_core::from_str(json);
        assert!(result.is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn call_result_error_uses_discriminant_five_not_four() {
        use crate::v201::RpcErrorCode;

        let error: CallResultError<RpcErrorCode> = CallResultError {
            message_id: MessageId::try_from("1").unwrap(),
            error_code: RpcErrorCode::FormatViolation,
            error_description: heapless::String::try_from("").unwrap(),
            error_details: EmptyPayload,
        };

        let mut buf = [0u8; 128];
        let len = serde_json_core::to_slice(&error, &mut buf).unwrap();
        assert!(core::str::from_utf8(&buf[..len]).unwrap().starts_with("[5,"));

        // A CALLERROR-shaped (discriminant 4) frame must NOT parse as a
        // CallResultError, even though every other field lines up.
        let call_error_json = r#"[4,"1","FormatViolation","",{}]"#;
        let result: Result<(CallResultError<RpcErrorCode>, usize), _> =
            serde_json_core::from_str(call_error_json);
        assert!(result.is_err());
    }

    #[cfg(all(feature = "serde", not(feature = "alloc")))]
    #[test]
    fn send_message_serializes_and_round_trips_a_real_send_type_message() {
        // `NotifyPeriodicEventStream` (OCPP 2.1) has no Request/Response
        // suffix at all in the spec -- it's a genuine SEND-only message,
        // not a CALL with an unusual name.
        //
        // This runs on a manually-spawned thread with a larger stack:
        // deserializing an envelope wrapping a type with two large
        // const-generic-defaulted fields (a 1024-byte string, a 16-item
        // vec) goes through several layers of serde's Visitor pattern,
        // and in an unoptimized debug build (no inlining) that's enough
        // to overflow the default test-thread stack. Release builds are
        // unaffected -- verified separately -- so this is purely a
        // `cargo test`-in-debug-mode artifact, not a real embedded-target
        // concern, but the test still needs to not crash in CI.
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                use crate::v21::NotifyPeriodicEventStream;

                let send: SendMessage<NotifyPeriodicEventStream> = SendMessage {
                    message_id: MessageId::try_from("19223201").unwrap(),
                    payload: NotifyPeriodicEventStream {
                        basetime: heapless::String::try_from("2024-08-27T12:30:40Z").unwrap(),
                        custom_data: None,
                        data: heapless::Vec::new(),
                        id: 123,
                        pending: 0,
                    },
                };

                let mut buf = [0u8; 256];
                let len = serde_json_core::to_slice(&send, &mut buf).unwrap();
                let json = core::str::from_utf8(&buf[..len]).unwrap();
                assert!(json.starts_with(r#"[6,"19223201","NotifyPeriodicEventStream","#));

                let (parsed, _): (SendMessage<NotifyPeriodicEventStream>, usize) =
                    serde_json_core::from_slice(&buf[..len]).unwrap();
                assert_eq!(parsed, send);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn message_id_round_trips_a_short_string() {
        let id = MessageId::try_from("19223201").unwrap();
        assert_eq!(id.as_str(), "19223201");
    }

    #[test]
    fn message_id_rejects_strings_over_36_bytes() {
        let too_long = "a".repeat(37);
        assert!(MessageId::try_from(too_long.as_str()).is_err());
    }

    #[test]
    fn message_id_accepts_exactly_36_bytes() {
        let exactly_36 = "a".repeat(36);
        assert!(MessageId::try_from(exactly_36.as_str()).is_ok());
    }
}
