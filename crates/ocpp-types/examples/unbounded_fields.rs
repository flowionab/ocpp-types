//! Fields with no spec-given bound (no `maxLength`/`maxItems`): a caller-
//! chosen const generic by default, or a plain `alloc` collection with the
//! `alloc` feature enabled.
//!
//! Run with: `cargo run --example unbounded_fields -p ocpp-types`
//! Or:       `cargo run --example unbounded_fields -p ocpp-types --features alloc`

use ocpp_types::v16::HeartbeatResponse;

fn main() {
    #[cfg(not(feature = "alloc"))]
    {
        // Uses the default capacity (1024).
        let response: HeartbeatResponse = HeartbeatResponse {
            current_time: heapless::String::try_from("2024-01-01T00:00:00Z").unwrap(),
        };
        println!("default capacity: {}", response.current_time.capacity());

        // Or pick a smaller one explicitly.
        let response: HeartbeatResponse<64> = HeartbeatResponse {
            current_time: heapless::String::try_from("2024-01-01T00:00:00Z").unwrap(),
        };
        println!("chosen capacity: {}", response.current_time.capacity());
    }

    #[cfg(feature = "alloc")]
    {
        // With `alloc`, the const generic disappears entirely -- this is
        // a plain, growable string (`alloc::string::String`, the same type
        // as `std::string::String`).
        let response = HeartbeatResponse {
            current_time: String::from("2024-01-01T00:00:00Z"),
        };
        println!("alloc mode, no capacity limit: {}", response.current_time);
    }
}
