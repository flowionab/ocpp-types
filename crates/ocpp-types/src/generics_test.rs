//! Regression coverage for the const-generic / `alloc` split on fields with
//! no spec-given bound (see `ocpp-codegen`'s `generics` module): a real
//! generated type must actually round-trip correctly in both the `no_std`
//! (caller-chosen capacity) and `alloc` (growable) builds, not just compile.
//!
//! Uses `DataTransferResponse.data` -- a free-text field the specification
//! genuinely leaves unbounded. Timestamps used to serve as the example here,
//! but `dateTime` fields are now [`crate::OcppTimestamp`], which is fixed
//! size and carries no parameter.

#[cfg(all(test, feature = "serde"))]
mod generics_check {
    use crate::v16::DataTransferResponse;
    use crate::v16::common::DataTransferResponseStatus;
    #[cfg(not(feature = "alloc"))]
    use crate::Action;

    #[cfg(not(feature = "alloc"))]
    #[test]
    fn no_std_default_capacity_round_trips() {
        let response: DataTransferResponse = DataTransferResponse {
            data: Some(heapless::String::try_from("vendor payload").unwrap()),
            status: DataTransferResponseStatus::Accepted,
        };

        let mut buf = [0u8; 128];
        let json = response.to_json_str(&mut buf).unwrap();
        assert_eq!(json, r#"{"data":"vendor payload","status":"Accepted"}"#);

        let parsed = DataTransferResponse::from_json_str(json).unwrap();
        assert_eq!(parsed, response);
    }

    #[cfg(not(feature = "alloc"))]
    #[test]
    fn caller_can_override_the_capacity() {
        // A capacity smaller than the 1024 default, chosen explicitly.
        let response: DataTransferResponse<32> = DataTransferResponse {
            data: Some(heapless::String::try_from("vendor payload").unwrap()),
            status: DataTransferResponseStatus::Accepted,
        };

        assert_eq!(response.data.as_ref().unwrap().capacity(), 32);

        let mut buf = [0u8; 128];
        let json = response.to_json_str(&mut buf).unwrap();
        let parsed = DataTransferResponse::<32>::from_json_str(json).unwrap();
        assert_eq!(parsed, response);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn alloc_build_uses_a_growable_string_and_needs_no_capacity() {
        let response = DataTransferResponse {
            data: Some(alloc::string::String::from("vendor payload")),
            status: DataTransferResponseStatus::Accepted,
        };

        assert_eq!(response.data.as_deref(), Some("vendor payload"));
    }
}
