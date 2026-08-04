//! Regression coverage for the const-generic / `alloc` split on fields with
//! no spec-given bound (see `ocpp-codegen`'s `generics` module): a real
//! generated type must actually round-trip correctly in both the `no_std`
//! (caller-chosen capacity) and `alloc` (growable) builds, not just compile.

#[cfg(all(test, feature = "serde"))]
mod generics_check {
    use crate::v16::HeartbeatResponse;
    #[cfg(not(feature = "alloc"))]
    use crate::Action;

    #[cfg(not(feature = "alloc"))]
    #[test]
    fn no_std_default_capacity_round_trips() {
        let response: HeartbeatResponse = HeartbeatResponse {
            current_time: heapless::String::try_from("2024-01-01T00:00:00Z").unwrap(),
        };

        let mut buf = [0u8; 128];
        let json = response.to_json_str(&mut buf).unwrap();
        assert_eq!(json, r#"{"currentTime":"2024-01-01T00:00:00Z"}"#);

        let parsed = HeartbeatResponse::from_json_str(json).unwrap();
        assert_eq!(parsed, response);
    }

    #[cfg(not(feature = "alloc"))]
    #[test]
    fn caller_can_override_the_capacity() {
        // A capacity smaller than the 1024 default, chosen explicitly.
        let response: HeartbeatResponse<32> = HeartbeatResponse {
            current_time: heapless::String::try_from("2024-01-01T00:00:00Z").unwrap(),
        };

        assert_eq!(response.current_time.capacity(), 32);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn alloc_mode_uses_a_real_growable_string_and_round_trips() {
        let response = HeartbeatResponse {
            current_time: alloc::string::String::from("2024-01-01T00:00:00Z"),
        };

        let json = serde_json_core::to_string::<_, 128>(&response).unwrap();
        assert_eq!(json.as_str(), r#"{"currentTime":"2024-01-01T00:00:00Z"}"#);

        let (parsed, _): (HeartbeatResponse, usize) =
            serde_json_core::from_str(json.as_str()).unwrap();
        assert_eq!(parsed, response);
    }
}
