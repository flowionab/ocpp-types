//! Tests for the generated `standard` modules -- the value sets the
//! specification defines for fields its JSON schemas type as bare strings.
//!
//! These are properties of the *generated* output, so they guard the
//! generator (and the vendored CSV tables it reads) rather than any
//! hand-written code: a bad export or a naming-rule change shows up here.

/// Asserts the three properties every generated value set must hold, for one
/// enum: `as_str` agrees with `Display`, `from_wire` inverts `as_str` for
/// every value, and no two values collide on the wire.
macro_rules! assert_value_set {
    ($ty:ty, max_len = $max:expr) => {{
        use core::str::FromStr;

        let all = <$ty>::ALL;

        assert!(!all.is_empty(), "{} has no values", stringify!($ty));

        for value in all {
            let wire = value.as_str();

            // Compared through references, since an *open* value set is not
            // `Copy` (its `Other` variant owns a string).
            assert_eq!(
                <$ty>::from_wire(wire).as_ref(),
                Some(value),
                "{}::{:?} does not round-trip through `{}`",
                stringify!($ty),
                value,
                wire
            );

            assert_eq!(
                &<$ty>::from_str(wire).unwrap(),
                value,
                "{}::{:?} does not round-trip through `FromStr`",
                stringify!($ty),
                value
            );

            assert!(!wire.is_empty(), "{} has an empty value", stringify!($ty));

            // The schema bounds each of these fields with a `maxLength`. A
            // standardized value that doesn't fit its own field would be
            // unsendable, so this catches a bad export as well as a wrong
            // bound in the table below.
            assert!(
                wire.len() <= $max,
                "{}::{:?} is {} bytes, over the schema's maxLength of {} for its field",
                stringify!($ty),
                value,
                wire.len(),
                $max
            );
        }

        // Two variants sharing a wire string would make `from_wire`
        // lossy -- it can only ever return one of them.
        for (index, value) in all.iter().enumerate() {
            for other in &all[index + 1..] {
                assert_ne!(
                    value.as_str(),
                    other.as_str(),
                    "{}: {:?} and {:?} share a wire value",
                    stringify!($ty),
                    value,
                    other
                );
            }
        }
    }};
}

mod v16 {
    use crate::v16::standard::*;

    #[test]
    fn every_value_set_round_trips_and_fits_its_field() {
        // `ChangeConfiguration.key` / `GetConfiguration.key`.
        assert_value_set!(ConfigurationKey, max_len = 50);
        // The Security Whitepaper's `SecurityEventNotification.type`.
        assert_value_set!(SecurityEvent, max_len = 50);
    }

    /// A spot-check that the transcription covers each feature profile,
    /// including the Security Whitepaper's additions -- these tables were
    /// authored from spec prose rather than exported, so a whole profile
    /// going missing is a realistic failure.
    #[test]
    fn covers_every_feature_profile_including_the_security_whitepaper() {
        for (key, profile) in [
            ("HeartbeatInterval", "Core"),
            ("LocalAuthListEnabled", "LocalAuthListManagement"),
            ("ReserveConnectorZeroSupported", "Reservation"),
            ("ChargeProfileMaxStackLevel", "SmartCharging"),
            ("SecurityProfile", "Security"),
        ] {
            let resolved = ConfigurationKey::from_wire(key)
                .unwrap_or_else(|| panic!("`{key}` missing from ConfigurationKey"));

            assert_eq!(resolved.feature_profile(), Some(profile), "for `{key}`");
        }
    }

    #[test]
    fn configuration_key_metadata_carries_the_spec_table_through() {
        let heartbeat = ConfigurationKey::from_wire("HeartbeatInterval").unwrap();

        assert_eq!(heartbeat.accessibility(), Some("RW"));
        assert_eq!(heartbeat.value_type(), Some("integer"));
        assert!(heartbeat.is_required());

        // A read-only, optional key, to check both columns actually vary.
        let blink = ConfigurationKey::from_wire("BlinkRepeat").unwrap();

        assert!(!blink.is_required());
        assert_eq!(
            ConfigurationKey::from_wire("NumberOfConnectors").unwrap().accessibility(),
            Some("R")
        );
    }

    /// 1.6's own naming is inconsistent here -- singular `MeterValue` in one
    /// key, plural `MeterValues` in its neighbours -- and getting it wrong
    /// produces a key no Charge Point recognizes.
    #[test]
    fn preserves_the_specs_inconsistent_metervalue_spellings() {
        assert!(ConfigurationKey::from_wire("MeterValueSampleInterval").is_some());
        assert!(ConfigurationKey::from_wire("MeterValuesSampledData").is_some());
        assert!(ConfigurationKey::from_wire("MeterValuesAlignedData").is_some());

        // The plural/singular swap of either must not silently resolve.
        assert!(ConfigurationKey::from_wire("MeterValuesSampleInterval").is_none());
        assert!(ConfigurationKey::from_wire("MeterValueSampledData").is_none());
    }

    /// 2.0.1 added this one alongside certificate-renewal flows 1.6 has no
    /// equivalent for, so it must not have been copied across.
    #[test]
    fn omits_the_two_oh_one_only_security_event() {
        assert_eq!(SecurityEvent::from_wire("DiscardedRenewedClientCertificate"), None);
    }

    #[test]
    fn security_event_criticality_varies() {
        assert!(SecurityEvent::FirmwareUpdated.is_critical());
        assert!(!SecurityEvent::from_wire("InvalidMessages").unwrap().is_critical());
    }
}

mod v201 {
    use crate::v201::standard::*;

    /// `maxLength` for each value set's wire field, from the 2.0.1 schemas.
    #[test]
    fn every_value_set_round_trips_and_fits_its_field() {
        assert_value_set!(ComponentName, max_len = 50); // Component.name
        assert_value_set!(VariableName, max_len = 50); // Variable.name
        assert_value_set!(SecurityEvent, max_len = 50); // SecurityEventNotificationRequest.type
        assert_value_set!(ReasonCode, max_len = 20); // StatusInfo.reasonCode
        assert_value_set!(Unit, max_len = 20); // UnitOfMeasure.unit
    }

    /// `VariableName` is an *open* set, so a vendor value parses into
    /// `Other` rather than failing -- but `from_wire` still reports it as not
    /// standardized, which is what a conformance check wants.
    #[test]
    fn a_vendor_value_lands_in_other_and_is_not_reported_as_standardized() {
        use core::str::FromStr;

        assert_eq!(VariableName::from_wire("AcmeVendorSpecific"), None);

        let parsed = VariableName::from_str("AcmeVendorSpecific").unwrap();

        assert_eq!(parsed.as_str(), "AcmeVendorSpecific");
        assert!(!parsed.is_standardized());
        assert!(VariableName::from_wire("HeartbeatInterval").unwrap().is_standardized());
    }

    /// A closed set keeps the stricter contract: unknown values are an error.
    #[test]
    fn a_closed_value_set_still_rejects_an_unknown_value() {
        use core::str::FromStr;

        assert_eq!(ReasonCode::from_wire("AcmeReason"), None);
        assert_eq!(ReasonCode::from_str("AcmeReason"), Err(UnknownValue));
    }

    /// The supplied 2.0.1 tables were partly 2.1 exports, which put
    /// 2.1-only names into this version's enums. Each name below belongs to
    /// a feature whose messages exist only in 2.1's schemas, so its presence
    /// here means the 2.0.1 source data has regressed to a 2.1 copy again.
    #[test]
    fn contains_no_component_whose_feature_is_two_one_only() {
        for (component, feature) in [
            ("ACDERCtrlr", "DER control"),
            ("DCDERCtrlr", "DER control"),
            ("BatterySwapCtrlr", "battery swap"),
            ("BatteryCartridge", "battery swap"),
            ("WebPaymentsCtrlr", "web payments"),
            ("V2XChargingCtrlr", "AFRR / V2X"),
            ("PaymentCtrlr", "ad hoc payment"),
        ] {
            assert_eq!(
                ComponentName::from_wire(component),
                None,
                "`{component}` is a 2.1 {feature} component and must not appear in 2.0.1"
            );
        }
    }

    #[test]
    fn contains_no_variable_that_two_one_introduced() {
        for variable in [
            "MaxPeriodicEventStreams",
            "SupportsEvseSleep",
            "WorkingMode",
            "UpstreamInterval",
            "HandleFailedTariff",
            "SetpointPriority",
        ] {
            assert_eq!(
                VariableName::from_wire(variable),
                None,
                "`{variable}` is listed only in 2.1's variables.csv and must not appear in 2.0.1"
            );
        }
    }

    /// One device-model row spelled this component `TxCtlrl` against ten
    /// correctly-spelled siblings, which generated a variant no station
    /// would ever send.
    #[test]
    fn does_not_carry_the_txctrlr_typo() {
        assert_eq!(ComponentName::from_wire("TxCtlrl"), None);
        assert!(ComponentName::from_wire("TxCtrlr").is_some());
    }

    #[test]
    fn device_model_table_is_populated_and_uses_none_for_generic_rows() {
        assert!(DEVICE_MODEL.len() > 300, "{}", DEVICE_MODEL.len());
        assert!(DEVICE_MODEL.iter().any(|entry| entry.component.is_none()));
        assert!(DEVICE_MODEL.iter().all(|entry| !entry.variable.is_empty()));
    }

    /// The device-model table's `Variable(Attr)` notation must have been
    /// split, or the table carries variable names no station ever sends.
    #[test]
    fn no_device_model_variable_carries_attribute_notation() {
        for entry in DEVICE_MODEL {
            assert!(
                !entry.variable.contains('('),
                "`{}` still carries attribute notation",
                entry.variable
            );
        }
    }

    /// Every name the device-model table references must exist in the
    /// corresponding enum, otherwise the two disagree about what the spec
    /// defines.
    #[test]
    fn every_device_model_name_is_a_known_component_and_variable() {
        for entry in DEVICE_MODEL {
            if let Some(component) = entry.component {
                assert!(
                    ComponentName::from_wire(component).is_some(),
                    "device model references unknown component `{component}`"
                );
            }

            assert!(
                VariableName::from_wire(entry.variable).is_some(),
                "device model references unknown variable `{}`",
                entry.variable
            );
        }
    }
}

mod v21 {
    use crate::v21::standard::*;

    #[test]
    fn every_value_set_round_trips_and_fits_its_field() {
        assert_value_set!(ComponentName, max_len = 50); // Component.name
        assert_value_set!(VariableName, max_len = 50); // Variable.name
        assert_value_set!(SecurityEvent, max_len = 50); // SecurityEventNotificationRequest.type
        assert_value_set!(ReasonCode, max_len = 20); // StatusInfo.reasonCode
        assert_value_set!(Unit, max_len = 20); // UnitOfMeasure.unit
        assert_value_set!(ConnectorType, max_len = 20); // connectorType
        assert_value_set!(IdTokenType, max_len = 20); // IdToken.type
        assert_value_set!(ChargingLimitSource, max_len = 20); // chargingLimitSource
        assert_value_set!(PaymentBrand, max_len = 20); // paymentBrand
        assert_value_set!(PaymentRecognition, max_len = 20); // paymentRecognition
        assert_value_set!(SigningMethod, max_len = 50); // signingMethod
        assert_value_set!(AdditionalInfoType, max_len = 50); // AdditionalInfo.type
    }

    /// `s309-1P-16A` and friends can't be Rust identifiers, so the variant
    /// is sanitized -- the wire value must survive that intact.
    #[test]
    fn a_sanitized_variant_keeps_its_punctuated_wire_value() {
        let connector = ConnectorType::from_wire("s309-1P-16A").expect("s309-1P-16A");

        assert_eq!(connector.as_str(), "s309-1P-16A");

        let method = SigningMethod::from_wire("ECDSA-secp256k1-SHA256").expect("ECDSA");

        assert_eq!(method.as_str(), "ECDSA-secp256k1-SHA256");
        assert_eq!(method.curve(), Some("secp256k1"));
        assert_eq!(method.hash_algorithm(), Some("SHA-256"));
    }

    #[test]
    fn metadata_accessors_carry_the_spec_table_through() {
        // `security_events.csv` marks this one critical.
        assert!(SecurityEvent::FirmwareUpdated.is_critical());

        let heartbeat = VariableName::from_wire("HeartbeatInterval").expect("HeartbeatInterval");

        assert_eq!(heartbeat.data_type(), Some("integer"));
        assert_eq!(heartbeat.unit(), Some("s"));
    }

    /// A value only present in the device-model table still has to be
    /// reachable: these are the configuration variables callers use most.
    #[test]
    fn variables_listed_only_in_the_device_model_table_are_present() {
        for name in ["OcppCsmsUrl", "VpnServer", "WebSocketPingInterval", "Apn"] {
            assert!(
                VariableName::from_wire(name).is_some(),
                "`{name}` is in the device model table but missing from VariableName"
            );
        }
    }

    #[test]
    fn every_device_model_name_is_a_known_component_and_variable() {
        for entry in DEVICE_MODEL {
            if let Some(component) = entry.component {
                assert!(
                    ComponentName::from_wire(component).is_some(),
                    "device model references unknown component `{component}`"
                );
            }

            assert!(
                VariableName::from_wire(entry.variable).is_some(),
                "device model references unknown variable `{}`",
                entry.variable
            );
        }
    }

    /// The point of the design: standardized names are additive, so a
    /// vendor-specific name still constructs a valid wire type.
    #[test]
    fn a_standard_name_and_a_vendor_name_both_build_a_variable() {
        use crate::v21::common::Variable;

        let standard: Variable = Variable {
            custom_data: None,
            instance: None,
            name: heapless::String::try_from(VariableName::HeartbeatInterval.as_str()).unwrap(),
        };

        let vendor: Variable = Variable {
            custom_data: None,
            instance: None,
            name: heapless::String::try_from("AcmeProprietaryKnob").unwrap(),
        };

        assert_eq!(standard.name.as_str(), "HeartbeatInterval");
        assert_eq!(vendor.name.as_str(), "AcmeProprietaryKnob");
        assert_eq!(VariableName::from_wire(vendor.name.as_str()), None);
    }
}

/// The `Other` arm on the open value sets: `ConfigurationKey` (1.6),
/// `ComponentName` and `VariableName` (2.0.1/2.1).
mod open_value_sets {
    use crate::v21::standard::{ComponentName, ValueTooLong, VariableName};

    /// The reason `Other` exists: a third-party value must survive being read
    /// and written back unchanged.
    #[test]
    fn a_vendor_value_round_trips_unchanged() {
        let parsed = VariableName::from_wire_or_other("AcmeChargeCurveTuning").unwrap();

        assert_eq!(parsed.as_str(), "AcmeChargeCurveTuning");
        assert_eq!(
            VariableName::from_wire_or_other(parsed.as_str()).unwrap(),
            parsed
        );
    }

    /// The invariant that makes `Other` safe to hand out: one wire string is
    /// one value, however it was built. Deriving `PartialEq` would make these
    /// unequal and quietly break lookups.
    #[test]
    fn other_holding_a_standardized_value_equals_that_variant() {
        let via_other = VariableName::Other(
            heapless::String::try_from("HeartbeatInterval").unwrap(),
        );
        let standardized = VariableName::from_wire("HeartbeatInterval").unwrap();

        assert_eq!(via_other, standardized);
        assert_eq!(via_other.as_str(), standardized.as_str());

        // ...and the same for ordering, which shares the comparison.
        assert_eq!(via_other.cmp(&standardized), core::cmp::Ordering::Equal);
    }

    /// `from_wire_or_other` normalizes, so callers who use it never construct
    /// the duplicate representation in the first place.
    #[test]
    fn from_wire_or_other_prefers_the_standardized_variant() {
        let parsed = VariableName::from_wire_or_other("HeartbeatInterval").unwrap();

        assert!(parsed.is_standardized());
        // Metadata is only reachable on the standardized variant, which is
        // the practical reason normalizing matters.
        assert_eq!(parsed.unit(), Some("s"));
    }

    /// `Other` carries no spec metadata, and must say so rather than claim a
    /// default.
    #[test]
    fn other_reports_no_metadata() {
        let vendor = VariableName::from_wire_or_other("AcmeKnob").unwrap();

        assert_eq!(vendor.data_type(), None);
        assert_eq!(vendor.unit(), None);
    }

    /// The bound is the field's own `maxLength`, so anything the wire field
    /// could carry fits -- and anything longer was never valid.
    #[test]
    fn a_value_over_the_fields_max_length_is_rejected_not_truncated() {
        let at_limit = "A".repeat(50);
        let over_limit = "A".repeat(51);

        assert_eq!(
            ComponentName::from_wire_or_other(&at_limit).unwrap().as_str(),
            at_limit
        );
        assert_eq!(
            ComponentName::from_wire_or_other(&over_limit),
            Err(ValueTooLong)
        );
    }

    /// `ALL` stays the standardized set -- it cannot enumerate `Other`.
    #[test]
    fn all_lists_only_standardized_values() {
        assert!(ComponentName::ALL.iter().all(ComponentName::is_standardized));
    }
}

#[cfg(feature = "serde")]
mod open_value_sets_serde {
    use crate::v16::standard::ConfigurationKey;
    use crate::v21::standard::VariableName;

    #[test]
    fn a_vendor_value_deserializes_instead_of_failing() {
        let (parsed, _): (VariableName, _) =
            serde_json_core::from_str(r#""AcmeVendorKnob""#).unwrap();

        assert_eq!(parsed.as_str(), "AcmeVendorKnob");
        assert!(!parsed.is_standardized());
    }

    #[test]
    fn a_standardized_value_still_deserializes_to_its_variant() {
        let (parsed, _): (VariableName, _) =
            serde_json_core::from_str(r#""HeartbeatInterval""#).unwrap();

        assert!(parsed.is_standardized());
        assert_eq!(parsed, VariableName::from_wire("HeartbeatInterval").unwrap());
    }

    #[test]
    fn both_kinds_serialize_back_to_their_wire_string() {
        let mut buf = [0u8; 64];

        let standardized = ConfigurationKey::from_wire("HeartbeatInterval").unwrap();
        let len = serde_json_core::to_slice(&standardized, &mut buf).unwrap();
        assert_eq!(core::str::from_utf8(&buf[..len]).unwrap(), r#""HeartbeatInterval""#);

        let vendor = ConfigurationKey::from_wire_or_other("AcmeVendorKey").unwrap();
        let len = serde_json_core::to_slice(&vendor, &mut buf).unwrap();
        assert_eq!(core::str::from_utf8(&buf[..len]).unwrap(), r#""AcmeVendorKey""#);
    }

    /// The 1.6 configuration keys are the case that motivated this: a Charge
    /// Point reporting its own keys in `GetConfiguration.conf` must not make
    /// the whole response undeserializable.
    #[test]
    fn a_mixed_list_of_standard_and_vendor_keys_deserializes_whole() {
        let (keys, _): (heapless::Vec<ConfigurationKey, 4>, _) =
            serde_json_core::from_str(
                r#"["HeartbeatInterval","AcmeVendorKey","MeterValueSampleInterval"]"#,
            )
            .unwrap();

        assert_eq!(keys.len(), 3);
        assert!(keys[0].is_standardized());
        assert!(!keys[1].is_standardized());
        assert!(keys[2].is_standardized());
        assert_eq!(keys[1].as_str(), "AcmeVendorKey");
    }
}

#[cfg(feature = "serde")]
mod serde_round_trip {
    use crate::v21::standard::{ConnectorType, SecurityEvent, VariableName};

    /// Values needing a `serde(rename)` are the ones a missing rename would
    /// silently break, so check one of each shape: clean, leading-lowercase,
    /// and punctuated.
    #[test]
    fn value_sets_serialize_to_their_wire_strings() {
        let mut buf = [0u8; 64];

        let len = serde_json_core::to_slice(&SecurityEvent::FirmwareUpdated, &mut buf).unwrap();
        assert_eq!(core::str::from_utf8(&buf[..len]).unwrap(), "\"FirmwareUpdated\"");

        let punctuated = ConnectorType::from_wire("s309-1P-16A").unwrap();
        let len = serde_json_core::to_slice(&punctuated, &mut buf).unwrap();
        assert_eq!(core::str::from_utf8(&buf[..len]).unwrap(), "\"s309-1P-16A\"");

        let (parsed, _): (ConnectorType, _) =
            serde_json_core::from_str("\"s309-1P-16A\"").unwrap();
        assert_eq!(parsed, punctuated);

        let (parsed, _): (VariableName, _) =
            serde_json_core::from_str("\"HeartbeatInterval\"").unwrap();
        assert_eq!(parsed, VariableName::from_wire("HeartbeatInterval").unwrap());
    }
}
