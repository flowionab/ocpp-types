//! `Validate` against the real generated messages, rather than the
//! generator's own token-level tests.
//!
//! The point of these is the *end* of the pipeline: a schema constraint
//! that survives parsing, code generation and compilation, and rejects the
//! payload it was meant to reject.

use crate::validate::{ConstraintClass, PathSegment, Validate, ValidationErrorKind};

/// A conformant `AuthorizeRequest`, as the starting point for the cases
/// that break one thing about it.
#[cfg(feature = "alloc")]
fn authorize_request() -> crate::v201::AuthorizeRequest {
    use crate::v201::common::{IdToken, IdTokenEnum};

    crate::v201::AuthorizeRequest {
        certificate: None,
        custom_data: None,
        id_token: IdToken {
            additional_info: None,
            custom_data: None,
            id_token: heapless::String::try_from("ABC123").unwrap(),
            r#type: IdTokenEnum::ISO14443,
        },
        iso15118_certificate_hash_data: None,
    }
}

#[cfg(feature = "alloc")]
#[test]
fn a_conformant_message_validates() {
    assert!(authorize_request().validate().is_ok());
}

/// The case the feature exists for. `certificate` is `maxLength: 5500` in
/// the schema but too large to inline, so under `alloc` it is a growable
/// `String` and the type enforces nothing.
#[cfg(feature = "alloc")]
#[test]
fn an_over_long_certificate_is_caught_even_though_the_type_accepted_it() {
    let mut request = authorize_request();
    request.certificate = Some("-".repeat(5501));

    let error = request.validate().unwrap_err();

    assert_eq!(
        error.kind(),
        ValidationErrorKind::TooLong {
            len: 5501,
            max: 5500
        }
    );
    assert_eq!(error.path(), &[PathSegment::Field("certificate")]);
}

#[cfg(feature = "alloc")]
#[test]
fn a_certificate_at_exactly_the_limit_is_accepted() {
    let mut request = authorize_request();
    request.certificate = Some("-".repeat(5500));

    assert!(request.validate().is_ok());
}

/// `minItems: 1` is the constraint no collection type can express -- a
/// `heapless::Vec` bounds the top end only, so an empty required array
/// compiles, serializes, and is rejected by the peer.
#[cfg(feature = "alloc")]
#[test]
fn an_empty_array_that_the_schema_requires_to_be_non_empty_is_caught() {
    let mut request = authorize_request();
    request.iso15118_certificate_hash_data = Some(heapless::Vec::new());

    let error = request.validate().unwrap_err();

    assert_eq!(
        error.kind(),
        ValidationErrorKind::TooFewItems { len: 0, min: 1 }
    );
    assert_eq!(error.kind().constraint_class(), ConstraintClass::Occurrence);
}

#[test]
fn an_integer_below_its_schema_minimum_is_caught() {
    let request: crate::v21::CancelReservationRequest = crate::v21::CancelReservationRequest {
        custom_data: None,
        reservation_id: -1,
    };

    let error = request.validate().unwrap_err();

    assert_eq!(
        error.kind(),
        ValidationErrorKind::BelowMinimum {
            value: -1.0,
            min: 0.0
        }
    );
    assert_eq!(error.path(), &[PathSegment::Field("reservationId")]);
    assert_eq!(error.kind().constraint_class(), ConstraintClass::Property);
}

/// 1.6J's charging limits are the only `multipleOf` in any version, and
/// they sit three levels down -- so this also covers the path a nested
/// failure reports.
#[cfg(feature = "alloc")]
#[test]
fn a_charging_limit_off_the_specs_step_is_caught_and_names_its_path() {
    use crate::v16::common::{
        ChargingProfileKind, ChargingProfilePurpose, ChargingRateUnit, ChargingSchedule,
        ChargingSchedulePeriodItem, CsChargingProfiles,
    };

    let request = crate::v16::SetChargingProfileRequest {
        connector_id: 1,
        cs_charging_profiles: CsChargingProfiles {
            charging_profile_id: 1,
            charging_profile_kind: ChargingProfileKind::Absolute,
            charging_profile_purpose: ChargingProfilePurpose::TxProfile,
            charging_schedule: ChargingSchedule {
                charging_rate_unit: ChargingRateUnit::A,
                charging_schedule_period: alloc::vec![
                    ChargingSchedulePeriodItem {
                        limit: 16.0,
                        number_phases: None,
                        start_period: 0,
                    },
                    ChargingSchedulePeriodItem {
                        limit: 16.15,
                        number_phases: None,
                        start_period: 900,
                    },
                ],
                duration: None,
                min_charging_rate: None,
                start_schedule: None,
            },
            recurrency_kind: None,
            stack_level: 0,
            transaction_id: None,
            valid_from: None,
            valid_to: None,
        },
    };

    let error = request.validate().unwrap_err();

    assert_eq!(
        error.kind(),
        ValidationErrorKind::NotMultipleOf {
            value: 16.15,
            multiple: 0.1
        }
    );
    assert_eq!(
        error.path(),
        &[
            PathSegment::Field("csChargingProfiles"),
            PathSegment::Field("chargingSchedule"),
            PathSegment::Field("chargingSchedulePeriod"),
            PathSegment::Index(1),
            PathSegment::Field("limit"),
        ]
    );
}

/// A whole tenth is conformant however badly it divides in binary -- the
/// first period above is `16.0`, and a naive remainder test would reject
/// plenty of others.
#[cfg(feature = "alloc")]
#[test]
fn charging_limits_on_the_specs_step_are_accepted() {
    use crate::validate::check_multiple_of;

    for limit in [0.0, 0.1, 6.0, 16.1, 32.0, 63.7, 100.5] {
        assert!(
            check_multiple_of(limit, 0.1).is_ok(),
            "{limit} is a whole number of tenths"
        );
    }
}

/// Without `alloc`, a caller-sized field can be given a capacity *above*
/// the spec's -- the const generic exists precisely so a deployment can
/// choose -- and nothing but this stops an over-long value going out.
#[cfg(not(feature = "alloc"))]
#[test]
fn a_capacity_chosen_above_the_spec_limit_is_still_validated_against_the_spec() {
    use crate::v201::common::{IdToken, IdTokenEnum};

    let request: crate::v201::AuthorizeRequest<crate::NoCustomData, 8000, 8> =
        crate::v201::AuthorizeRequest {
            // 8000 fits the type, and is 2500 characters past the schema.
            certificate: Some(heapless::String::try_from("-".repeat(6000).as_str()).unwrap()),
            custom_data: None,
            id_token: IdToken {
                additional_info: None,
                custom_data: None,
                id_token: heapless::String::try_from("ABC123").unwrap(),
                r#type: IdTokenEnum::ISO14443,
            },
            iso15118_certificate_hash_data: None,
        };

    let error = request.validate().unwrap_err();

    assert_eq!(
        error.kind(),
        ValidationErrorKind::TooLong {
            len: 6000,
            max: 5500
        }
    );
}
