//! Zero-allocation JSON (de)serialization via the [`Action`] trait.
//!
//! Run with: `cargo run --example serialization -p ocpp-types --features serde`

use ocpp_types::v16::{AuthorizeRequest, IdTag};
use ocpp_types::Action;

fn main() {
    let request = AuthorizeRequest {
        id_tag: IdTag::try_from("ABC123").expect("fits in 20 chars"),
    };

    // The caller owns the buffer -- nothing here allocates.
    let mut buf = [0u8; 256];
    let json: &str = request.to_json_str(&mut buf).expect("buffer is big enough");
    println!("serialized: {json}");

    let parsed = AuthorizeRequest::from_json_str(json).expect("valid JSON");
    assert_eq!(parsed, request);
    println!("round-tripped successfully: {parsed:?}");

    // `to_json_slice`/`from_json_slice` are the same, but for raw bytes
    // instead of `&str`.
    let mut byte_buf = [0u8; 256];
    let bytes: &[u8] = request.to_json_slice(&mut byte_buf).unwrap();
    println!("as bytes: {bytes:?}");
}
