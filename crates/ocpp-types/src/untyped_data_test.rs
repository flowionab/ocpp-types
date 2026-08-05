//! Regression coverage for schema properties with no declared type -- the
//! `data` field on 2.0.1/2.1's `DataTransfer`, which the spec deliberately
//! leaves "open to implementation" (see `ocpp-codegen`'s `RustType::Any`).
//! These used to be generated as `Option<()>`, which is structurally
//! incapable of carrying the vendor payload the message exists to transport;
//! they're now a caller-chosen type parameter, so the real check is that a
//! concrete payload actually round-trips on the wire.

#[cfg(all(test, feature = "serde"))]
mod untyped_data {
    use crate::Action;

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    struct VendorPayload {
        #[serde(rename = "sessionId")]
        session_id: heapless::String<16>,
        counter: i64,
    }

    fn payload() -> VendorPayload {
        VendorPayload {
            session_id: heapless::String::try_from("S-42").unwrap(),
            counter: 7,
        }
    }

    #[test]
    fn v201_request_round_trips_a_caller_chosen_payload() {
        use crate::v201::DataTransferRequest;

        let request: DataTransferRequest<VendorPayload> = DataTransferRequest {
            custom_data: None,
            data: Some(payload()),
            message_id: None,
            vendor_id: heapless::String::try_from("com.example").unwrap(),
        };

        let mut buf = [0u8; 256];
        let json = request.to_json_str(&mut buf).unwrap();
        assert_eq!(
            json,
            r#"{"data":{"sessionId":"S-42","counter":7},"vendorId":"com.example"}"#
        );

        let parsed = DataTransferRequest::<VendorPayload>::from_json_str(json).unwrap();
        assert_eq!(parsed, request);
    }

    #[test]
    fn v201_response_round_trips_a_caller_chosen_payload() {
        use crate::v201::common::DataTransferStatusEnum;
        use crate::v201::DataTransferResponse;

        let response: DataTransferResponse<VendorPayload> = DataTransferResponse {
            custom_data: None,
            data: Some(payload()),
            status: DataTransferStatusEnum::Accepted,
            status_info: None,
        };

        let mut buf = [0u8; 256];
        let json = response.to_json_str(&mut buf).unwrap();
        assert_eq!(
            json,
            r#"{"data":{"sessionId":"S-42","counter":7},"status":"Accepted"}"#
        );

        let parsed = DataTransferResponse::<VendorPayload>::from_json_str(json).unwrap();
        assert_eq!(parsed, response);
    }

    #[test]
    fn v21_request_round_trips_a_caller_chosen_payload() {
        use crate::v21::DataTransferRequest;

        let request: DataTransferRequest<VendorPayload> = DataTransferRequest {
            custom_data: None,
            data: Some(payload()),
            message_id: None,
            vendor_id: heapless::String::try_from("com.example").unwrap(),
        };

        let mut buf = [0u8; 256];
        let json = request.to_json_str(&mut buf).unwrap();
        let parsed = DataTransferRequest::<VendorPayload>::from_json_str(json).unwrap();

        assert_eq!(parsed, request);
    }

    /// A payload type isn't required: code that never sends `data` keeps
    /// working with the default `()`, exactly as it did before the field
    /// became generic.
    #[test]
    fn the_default_payload_type_still_omits_the_field_entirely() {
        use crate::v201::DataTransferRequest;

        let request: DataTransferRequest = DataTransferRequest {
            custom_data: None,
            data: None,
            message_id: None,
            vendor_id: heapless::String::try_from("com.example").unwrap(),
        };

        let mut buf = [0u8; 128];
        let json = request.to_json_str(&mut buf).unwrap();
        assert_eq!(json, r#"{"vendorId":"com.example"}"#);
    }

    /// The action name is unchanged by the payload type -- the `Action` impl
    /// is generic over it.
    #[test]
    fn action_name_is_available_for_any_payload_type() {
        use crate::v201::DataTransferRequest;

        assert_eq!(DataTransferRequest::<VendorPayload>::ACTION, "DataTransfer");
        assert_eq!(DataTransferRequest::<()>::ACTION, "DataTransfer");
    }
}
