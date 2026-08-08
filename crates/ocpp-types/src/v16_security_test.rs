//! Tests for the 1.6J Security Whitepaper messages.
//!
//! These schemas are authored rather than vendored (the OCA publishes no
//! JSON schemas for the whitepaper messages -- see
//! `schemas/ocpp1.6j/README.md`), so these tests pin the shapes that
//! authoring decided: which fields are required, which are optional, and
//! that each message reports the right OCPP action.

use crate::Action;
use crate::v16::*;
// Shared types reached via `$ref` live in `common`, not the version root.
use crate::v16::common::{
    CertificateHashData, ExtendedTriggerMessageRequestRequestedMessage, Firmware,
    GetInstalledCertificateIdsResponseStatus, HashAlgorithm, Log, LogType,
    TriggerMessageRequestRequestedMessage,
};

/// Builds the string type an over-threshold field uses in this feature
/// configuration: `heapless::String<N>` by default, `alloc::string::String`
/// under `alloc`.
///
/// Certificates, CSRs and signing certificates all state a `maxLength` above
/// the inline-string threshold, so they are caller-chosen capacities -- and
/// land on either side of the `alloc` split. Writing the tests through this
/// keeps them running in both configurations.
macro_rules! large_string {
    ($text:expr) => {{
        #[cfg(feature = "alloc")]
        {
            alloc::string::String::from($text)
        }
        #[cfg(not(feature = "alloc"))]
        {
            heapless::String::try_from($text).unwrap()
        }
    }};
}

/// Reads `ACTION` off a value rather than a type path.
///
/// Messages carrying an unbounded field (the whitepaper's `dateTime`s have
/// no `maxLength`) are generic over its capacity in the no-`alloc` build, so
/// `Message::ACTION` can't infer the const parameters -- but inference works
/// fine from a constructed value, and this way the same call compiles under
/// both feature configurations.
fn action_of<T: Action>(_message: &T) -> &'static str {
    T::ACTION
}

/// The action string is what a Central System dispatches on, so a typo here
/// makes the message unroutable no matter how correct its payload is.
#[test]
fn every_security_message_reports_its_action() {
    assert_eq!(DeleteCertificateRequest::ACTION, "DeleteCertificate");
    assert_eq!(ExtendedTriggerMessageRequest::ACTION, "ExtendedTriggerMessage");
    assert_eq!(
        GetInstalledCertificateIdsRequest::ACTION,
        "GetInstalledCertificateIds"
    );
    assert_eq!(LogStatusNotificationRequest::ACTION, "LogStatusNotification");
    assert_eq!(
        SignedFirmwareStatusNotificationRequest::ACTION,
        "SignedFirmwareStatusNotification"
    );

    // Responses share the action of the request they answer.
    assert_eq!(CertificateSignedResponse::ACTION, "CertificateSigned");
    assert_eq!(GetLogResponse::ACTION, "GetLog");
    assert_eq!(DeleteCertificateResponse::ACTION, "DeleteCertificate");
    assert_eq!(SecurityEventNotificationResponse::ACTION, "SecurityEventNotification");
    assert_eq!(
        SignedUpdateFirmwareResponse::ACTION,
        "SignedUpdateFirmware"
    );
    assert_eq!(
        ExtendedTriggerMessageResponse::ACTION,
        "ExtendedTriggerMessage"
    );
    assert_eq!(InstallCertificateResponse::ACTION, "InstallCertificate");
    assert_eq!(LogStatusNotificationResponse::ACTION, "LogStatusNotification");
    assert_eq!(SignCertificateResponse::ACTION, "SignCertificate");
    assert_eq!(
        SignedFirmwareStatusNotificationResponse::ACTION,
        "SignedFirmwareStatusNotification"
    );
}

/// The certificate-bearing requests carry a capacity parameter now that
/// large spec-bounded strings are caller-chosen, so their action is read
/// from a value rather than a type path.
#[test]
fn the_certificate_bearing_requests_report_their_actions() {
    let signed: CertificateSignedRequest = CertificateSignedRequest {
        certificate_chain: large_string!("-----BEGIN CERTIFICATE-----"),
    };
    let install: InstallCertificateRequest = InstallCertificateRequest {
        certificate_type: crate::v16::common::CertificateType::CentralSystemRootCertificate,
        certificate: large_string!("-----BEGIN CERTIFICATE-----"),
    };
    let sign: SignCertificateRequest = SignCertificateRequest {
        csr: large_string!("-----BEGIN CERTIFICATE REQUEST-----"),
    };

    assert_eq!(action_of(&signed), "CertificateSigned");
    assert_eq!(action_of(&install), "InstallCertificate");
    assert_eq!(action_of(&sign), "SignCertificate");
}

/// `DeleteCertificate` and `GetInstalledCertificateIds` both carry
/// `CertificateHashData`. It must be *one* type, or callers have to convert
/// between two identical structs to pass a hash from one message to another.
#[test]
fn certificate_hash_data_is_a_single_shared_type() {
    let hash = CertificateHashData {
        hash_algorithm: HashAlgorithm::SHA256,
        issuer_name_hash: heapless::String::try_from("a1b2").unwrap(),
        issuer_key_hash: heapless::String::try_from("c3d4").unwrap(),
        serial_number: heapless::String::try_from("0A1B").unwrap(),
    };

    // Same value satisfies the field on the request...
    let request = DeleteCertificateRequest {
        certificate_hash_data: hash.clone(),
    };

    // ...and an element of the response's list, with no conversion.
    let listed: heapless::Vec<CertificateHashData, 1> =
        heapless::Vec::from_slice(&[hash]).unwrap();

    assert_eq!(request.certificate_hash_data, listed[0]);
}

/// The whitepaper states no `maxItems` for this list, so it becomes a
/// const-generic capacity like every other unbounded field in the crate --
/// which also means its `ACTION` has to be read from a value.
#[test]
fn get_installed_certificate_ids_response_carries_an_unbounded_hash_list() {
    let response: GetInstalledCertificateIdsResponse = GetInstalledCertificateIdsResponse {
        status: GetInstalledCertificateIdsResponseStatus::Accepted,
        certificate_hash_data: None,
    };

    assert_eq!(action_of(&response), "GetInstalledCertificateIds");
    assert_eq!(
        response.status,
        GetInstalledCertificateIdsResponseStatus::Accepted
    );
}

#[test]
fn get_log_requires_only_log_log_type_and_request_id() {
    let request: GetLogRequest = GetLogRequest {
        log: Log {
            remote_location: heapless::String::try_from("ftp://example.test/logs").unwrap(),
            oldest_timestamp: None,
            latest_timestamp: None,
        },
        log_type: LogType::SecurityLog,
        request_id: 42,
        retries: None,
        retry_interval: None,
    };

    assert_eq!(action_of(&request), "GetLog");
    assert_eq!(request.log_type, LogType::SecurityLog);
    assert_eq!(request.request_id, 42);
}

#[test]
fn signed_update_firmware_carries_the_signature_and_signing_certificate() {
    let request: SignedUpdateFirmwareRequest = SignedUpdateFirmwareRequest {
        request_id: 7,
        firmware: Firmware {
            location: heapless::String::try_from("https://example.test/fw.bin").unwrap(),
            retrieve_date_time: crate::OcppTimestamp::parse_rfc3339("2026-01-01T00:00:00Z").unwrap(),
            install_date_time: None,
            signing_certificate: large_string!("-----BEGIN CERTIFICATE-----"),
            signature: large_string!("MEUCIQ"),
        },
        retries: None,
        retry_interval: None,
    };

    assert_eq!(action_of(&request), "SignedUpdateFirmware");
    assert_eq!(request.firmware.signature.as_str(), "MEUCIQ");
}

/// The whitepaper's trigger enum is not the core profile's: it adds
/// `SignChargePointCertificate` and `LogStatusNotification`, and drops
/// `DiagnosticsStatusNotification`. Sharing one enum between the two
/// messages would let a caller trigger something the other message can't
/// express.
#[test]
fn extended_trigger_has_its_own_message_enum_distinct_from_the_core_one() {
    let extended = ExtendedTriggerMessageRequest {
        requested_message: ExtendedTriggerMessageRequestRequestedMessage::SignChargePointCertificate,
        connector_id: None,
    };

    let core = TriggerMessageRequest {
        requested_message: TriggerMessageRequestRequestedMessage::BootNotification,
        connector_id: None,
    };

    assert_eq!(
        extended.requested_message,
        ExtendedTriggerMessageRequestRequestedMessage::SignChargePointCertificate
    );
    assert_eq!(
        core.requested_message,
        TriggerMessageRequestRequestedMessage::BootNotification
    );
}

#[test]
fn security_event_notification_type_field_is_a_raw_identifier() {
    let request: SecurityEventNotificationRequest = SecurityEventNotificationRequest {
        // `type` is a Rust keyword, so the generated field is `r#type`.
        r#type: heapless::String::try_from(
            crate::v16::standard::SecurityEvent::FirmwareUpdated.as_str(),
        )
        .unwrap(),
        timestamp: crate::OcppTimestamp::parse_rfc3339("2026-01-01T00:00:00Z").unwrap(),
        tech_info: None,
    };

    assert_eq!(action_of(&request), "SecurityEventNotification");
    assert_eq!(request.r#type.as_str(), "FirmwareUpdated");
}

#[cfg(feature = "serde")]
mod wire {
    use super::*;
    use crate::v16::common::{CertificateType, LogStatusNotificationRequestStatus};

    #[test]
    fn certificate_signed_round_trips_through_json() {
        let request: CertificateSignedRequest = CertificateSignedRequest {
            certificate_chain: large_string!("-----BEGIN CERTIFICATE-----"),
        };

        let mut buf = [0u8; 256];
        let json = request.to_json_str(&mut buf).unwrap();

        assert_eq!(
            json,
            r#"{"certificateChain":"-----BEGIN CERTIFICATE-----"}"#
        );
        assert_eq!(CertificateSignedRequest::from_json_str(json).unwrap(), request);
    }

    /// Optional fields must not appear when absent -- a Charge Point
    /// rejecting `null` for an omitted field is conformant behaviour.
    #[test]
    fn absent_optional_fields_are_omitted_not_null() {
        let notification = LogStatusNotificationRequest {
            status: LogStatusNotificationRequestStatus::Uploaded,
            request_id: None,
        };

        let mut buf = [0u8; 128];
        let json = notification.to_json_str(&mut buf).unwrap();

        assert_eq!(json, r#"{"status":"Uploaded"}"#);
    }

    #[test]
    fn empty_responses_serialize_as_an_empty_object() {
        let mut buf = [0u8; 32];

        let json = SecurityEventNotificationResponse {}
            .to_json_str(&mut buf)
            .unwrap();

        assert_eq!(json, "{}");
    }

    #[test]
    fn field_names_use_the_specs_camel_case_on_the_wire() {
        let request = GetInstalledCertificateIdsRequest {
            certificate_type: CertificateType::CentralSystemRootCertificate,
        };

        let mut buf = [0u8; 128];
        let json = request.to_json_str(&mut buf).unwrap();

        assert_eq!(
            json,
            r#"{"certificateType":"CentralSystemRootCertificate"}"#
        );
    }
}
