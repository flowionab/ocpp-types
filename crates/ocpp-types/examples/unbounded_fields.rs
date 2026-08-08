//! Fields with no spec-given bound (no `maxLength`/`maxItems`): a caller-
//! chosen const generic by default, or a plain `alloc` collection with the
//! `alloc` feature enabled.
//!
//! Run with: `cargo run --example unbounded_fields -p ocpp-types`
//! Or:       `cargo run --example unbounded_fields -p ocpp-types --features alloc`

use ocpp_types::v16::DataTransferResponse;
use ocpp_types::v16::common::DataTransferResponseStatus;

fn main() {
    #[cfg(not(feature = "alloc"))]
    {
        // Uses the default capacity (1024).
        let response: DataTransferResponse = DataTransferResponse {
            data: Some(heapless::String::try_from("vendor payload").unwrap()),
            status: DataTransferResponseStatus::Accepted,
        };
        println!(
            "default capacity: {}",
            response.data.as_ref().unwrap().capacity()
        );

        // Or pick a smaller one explicitly.
        let response: DataTransferResponse<64> = DataTransferResponse {
            data: Some(heapless::String::try_from("vendor payload").unwrap()),
            status: DataTransferResponseStatus::Accepted,
        };
        println!(
            "chosen capacity: {}",
            response.data.as_ref().unwrap().capacity()
        );
    }

    #[cfg(feature = "alloc")]
    {
        // With `alloc`, the const generic disappears entirely -- this is
        // a plain, growable string (`alloc::string::String`, the same type
        // as `std::string::String`).
        let response = DataTransferResponse {
            data: Some(String::from("vendor payload")),
            status: DataTransferResponseStatus::Accepted,
        };
        println!("alloc mode, no capacity limit: {:?}", response.data);
    }

    // Timestamps are *not* in this category any more. Every OCPP version
    // types them `{"type": "string", "format": "date-time"}` with no
    // `maxLength`, so they used to take the 1024-byte default too -- they
    // are now `OcppTimestamp`, which is 16 bytes and needs no parameter in
    // either build.
    let heartbeat = ocpp_types::v16::HeartbeatResponse {
        current_time: ocpp_types::OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
    };
    println!(
        "timestamp: {} ({} bytes)",
        heartbeat.current_time,
        core::mem::size_of::<ocpp_types::OcppTimestamp>()
    );
}
