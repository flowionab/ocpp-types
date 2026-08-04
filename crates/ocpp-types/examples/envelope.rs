//! OCPP-J WebSocket envelopes: `CALL`, `CALLRESULT`, `CALLERROR`.
//!
//! Run with: `cargo run --example envelope -p ocpp-types --features serde`

use ocpp_types::v16::{AuthorizeRequest, HeartbeatResponse, IdTag, RpcErrorCode};
use ocpp_types::{Call, CallError, CallResult, EmptyPayload, MessageId};

fn main() {
    // A CALL wraps a request with a correlation id. The wire's "Action"
    // string is derived from `AuthorizeRequest::ACTION`, not stored
    // redundantly -- and validated against it when parsing one back.
    let call = Call {
        message_id: MessageId::try_from("19223201").unwrap(),
        payload: AuthorizeRequest {
            id_tag: IdTag::try_from("ABC123").unwrap(),
        },
    };

    let mut buf = [0u8; 256];
    let len = serde_json_core::to_slice(&call, &mut buf).unwrap();
    let json = core::str::from_utf8(&buf[..len]).unwrap();
    println!("CALL:        {json}");

    let (parsed, _): (Call<AuthorizeRequest>, usize) =
        serde_json_core::from_slice(&buf[..len]).unwrap();
    assert_eq!(parsed, call);

    // A CALLRESULT correlates back to the CALL by `message_id` alone --
    // there's no Action on the wire for it, since the receiver already
    // knows (from tracking its own outstanding CALLs) what kind of
    // response to expect.
    #[cfg(not(feature = "alloc"))]
    let current_time = heapless::String::try_from("2024-01-01T00:00:00Z").unwrap();
    #[cfg(feature = "alloc")]
    let current_time = String::from("2024-01-01T00:00:00Z");

    let result: CallResult<HeartbeatResponse> = CallResult {
        message_id: MessageId::try_from("19223201").unwrap(),
        payload: HeartbeatResponse { current_time },
    };

    let mut buf = [0u8; 256];
    let len = serde_json_core::to_slice(&result, &mut buf).unwrap();
    println!("CALLRESULT:  {}", core::str::from_utf8(&buf[..len]).unwrap());

    // A CALLERROR carries the version's own RpcErrorCode. `errorDetails`
    // defaults to `EmptyPayload` ({}), since the spec leaves it
    // deliberately undefined in shape.
    let error: CallError<RpcErrorCode> = CallError {
        message_id: MessageId::try_from("19223201").unwrap(),
        error_code: RpcErrorCode::NotImplemented,
        error_description: heapless::String::try_from("unrecognized action").unwrap(),
        error_details: EmptyPayload,
    };

    let mut buf = [0u8; 256];
    let len = serde_json_core::to_slice(&error, &mut buf).unwrap();
    println!("CALLERROR:   {}", core::str::from_utf8(&buf[..len]).unwrap());

    // A frame with the wrong MessageTypeId or a mismatched Action is
    // rejected outright, not silently accepted.
    let wrong_type: Result<(Call<AuthorizeRequest>, usize), _> =
        serde_json_core::from_str(r#"[3,"1","Authorize",{"idTag":"ABC123"}]"#);
    println!(
        "a CALLRESULT-shaped frame parsed as Call<T> is rejected: {}",
        wrong_type.is_err()
    );
}
