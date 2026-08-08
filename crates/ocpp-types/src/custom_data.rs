//! The default type for OCPP 2.x's `customData` extension point.

/// Stands in for a `customData` object this deployment doesn't read.
///
/// 2.0.1 and 2.1 hang an optional `customData` field on nearly every object
/// in the specification -- 151 structs in 2.1 alone -- for vendor
/// extensions. Storing the spec's shape inline costs 264 bytes at *every*
/// one of those nodes, and because structs nest by value that cost is
/// multiplied by the whole type graph rather than paid once.
///
/// So the field's type is a parameter, and this is its default: a
/// zero-sized type, making `Option<NoCustomData>` one byte.
///
/// It is deliberately **permissive, not empty**. A peer may send
/// `customData` whenever it likes, so the default has to accept and discard
/// it -- `Option<()>` would only accept `null` and would fail to parse any
/// message that actually carried an extension. That would turn opting out
/// of custom data into an interop bug.
///
/// The trade is that discarded data is not echoed back: re-serializing a
/// value parsed into `NoCustomData` writes `{}`, not what arrived. A
/// deployment that needs the contents names its own type instead:
///
/// ```
/// # #[cfg(feature = "serde")] {
/// use ocpp_types::v21::common::{Component, CustomData};
///
/// // The specification's own shape, opted into:
/// let with_spec_shape: Component<CustomData> = Component {
///     custom_data: None,
///     evse: None,
///     instance: None,
///     name: heapless::String::try_from("EVSE").unwrap(),
/// };
/// # let _ = with_spec_shape;
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct NoCustomData;

#[cfg(feature = "serde")]
impl serde::Serialize for NoCustomData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        serializer.serialize_map(Some(0))?.end()
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for NoCustomData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = NoCustomData;

            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("a customData object")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                while map
                    .next_entry::<serde::de::IgnoredAny, serde::de::IgnoredAny>()?
                    .is_some()
                {}

                Ok(NoCustomData)
            }

            // A peer that sends `"customData": null` means the same thing as
            // omitting it.
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(NoCustomData)
            }
        }

        deserializer.deserialize_map(Visitor)
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use crate::Action;
    use crate::v21::HeartbeatRequest;
    use crate::v21::common::{Component, CustomData};

    /// The default has to *accept* a `customData` object, not just tolerate
    /// its absence: a peer may send one at any time, and `Option<()>` would
    /// only accept `null`. Rejecting it would turn opting out of custom data
    /// into an interop failure.
    #[test]
    fn the_default_discards_a_populated_custom_data_object() {
        let json = r#"{"customData":{"vendorId":"com.acme","extra":[1,2,3]},"name":"EVSE"}"#;
        let (component, _): (Component, _) = serde_json_core::from_str(json).unwrap();

        assert_eq!(component.name.as_str(), "EVSE");
        assert!(component.custom_data.is_some());
    }

    #[test]
    fn the_default_still_accepts_a_message_without_custom_data() {
        let (component, _): (Component, _) =
            serde_json_core::from_str(r#"{"name":"EVSE"}"#).unwrap();

        assert!(component.custom_data.is_none());
    }

    /// Opting back into the specification's own shape, which is what the
    /// generated `CustomData` struct is for.
    #[test]
    fn a_caller_can_name_the_specs_custom_data_shape_and_read_the_vendor_id() {
        let json = r#"{"customData":{"vendorId":"com.acme"},"name":"EVSE"}"#;
        let (component, _): (Component<CustomData>, _) =
            serde_json_core::from_str(json).unwrap();

        assert_eq!(component.custom_data.unwrap().vendor_id.as_str(), "com.acme");
    }

    /// A deployment with its own extension shape names that instead.
    #[test]
    fn a_caller_can_supply_an_entirely_custom_shape() {
        #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
        struct AcmeExtension {
            #[serde(rename = "vendorId")]
            vendor_id: heapless::String<16>,
            #[serde(rename = "siteId")]
            site_id: u32,
        }

        let json = r#"{"customData":{"vendorId":"com.acme","siteId":42},"name":"EVSE"}"#;
        let (component, _): (Component<AcmeExtension>, _) =
            serde_json_core::from_str(json).unwrap();

        let extension = component.custom_data.unwrap();
        assert_eq!(extension.site_id, 42);
        assert_eq!(extension.vendor_id.as_str(), "com.acme");
    }

    /// The parameter threads all the way to the message types, not just the
    /// nested structs -- otherwise a caller could never choose it.
    #[test]
    fn the_parameter_reaches_the_message_types() {
        let request: HeartbeatRequest<CustomData> = HeartbeatRequest {
            custom_data: Some(CustomData {
                vendor_id: heapless::String::try_from("com.acme").unwrap(),
            }),
        };

        assert_eq!(HeartbeatRequest::<CustomData>::ACTION, "Heartbeat");
        assert!(request.custom_data.is_some());
    }

    /// The whole point: one byte by default against 272 for the spec shape,
    /// paid at every node of the type graph rather than once. (272, not 264:
    /// `heapless::String` has no spare bit pattern, so `Option` cannot pack
    /// its discriminant into the payload.)
    #[test]
    fn the_default_costs_one_byte_and_the_spec_shape_costs_the_full_field() {
        assert_eq!(core::mem::size_of::<Option<super::NoCustomData>>(), 1);
        assert_eq!(core::mem::size_of::<Option<CustomData>>(), 272);
    }
}
