use crate::model::ConstParam;
use crate::model::GeneratedType;
use crate::model::OcppVersion;
use crate::model::ParsedSchema;
use crate::model::RustEnum;
use crate::model::RustField;
use crate::model::RustStruct;
use crate::model::RustType;
use crate::model::RustVariant;
use crate::pool::TypePool;
use serde_json::Map;
use serde_json::Value;
use std::collections::HashSet;

/// Default capacity for a `heapless::String<N>` backing a field with no
/// spec-given `maxLength`, when the caller doesn't override the const
/// generic parameter.
const DEFAULT_STRING_CAPACITY: usize = 1024;

/// Default capacity for a `heapless::Vec<T, N>` backing a field with no
/// spec-given `maxItems`, when the caller doesn't override the const
/// generic parameter. Smaller than the string default: OCPP arrays without
/// a stated bound (sampled values, schedule periods, tariff entries, ...)
/// are typically short lists, unlike free-text fields.
const DEFAULT_VEC_CAPACITY: usize = 16;

pub struct SchemaParser<'a> {
    definitions: Map<String, Value>,
    pool: &'a mut TypePool,
    version: OcppVersion,
    /// Names currently being resolved within *this* schema file, so a
    /// self-referential definition doesn't recurse forever. Distinct from
    /// `pool`, which tracks types that are already fully generated
    /// (possibly by an earlier schema file).
    in_progress: HashSet<String>,
}

impl<'a> SchemaParser<'a> {
    /// Parses a single schema file in isolation: nested types it pulls in
    /// via `$ref` are returned alongside it rather than shared with
    /// anything else. Use [`Self::parse_with_pool`] when generating many
    /// schema files together so shared definitions are deduplicated across
    /// them.
    pub fn parse(value: &Value, version: OcppVersion) -> anyhow::Result<ParsedSchema> {
        let mut pool = TypePool::new();
        let message = Self::parse_with_pool(value, &mut pool, version)?;

        Ok(ParsedSchema {
            message,
            types: pool.types().to_vec(),
        })
    }

    /// Parses a single schema file, registering any `$ref`-resolved types
    /// into `pool`. Calling this repeatedly with the same `pool` across
    /// multiple schema files is what gives cross-file deduplication: a
    /// definition already registered by an earlier file is reused instead
    /// of being generated again.
    pub fn parse_with_pool(
        value: &Value,
        pool: &mut TypePool,
        version: OcppVersion,
    ) -> anyhow::Result<RustStruct> {
        let definitions = value["definitions"]
            .as_object()
            .cloned()
            .unwrap_or_default();

        let mut parser = SchemaParser {
            definitions,
            pool,
            version,
            in_progress: HashSet::new(),
        };

        let name = Self::message_name(value)?;
        let action = Self::action_name(&name);
        let description = Self::description(value);
        let fields = parser.parse_fields(&name, value)?;

        Ok(RustStruct {
            name,
            action: Some(action),
            description,
            fields,
        })
    }

    /// A schema's `description`, sanitized for use as a doc comment:
    /// `\r\n` (common in these schemas) normalized to `\n`, leading/
    /// trailing whitespace trimmed, and `[`/`]` escaped since spec text
    /// like `[RFC5646]` (a citation) or `[0-9]` (a character class)
    /// otherwise reads as rustdoc intra-doc link syntax and produces
    /// broken-link warnings for links that were never meant to be links.
    /// `None` if absent or blank.
    fn description(schema: &Value) -> Option<String> {
        let raw = schema["description"].as_str()?;
        let normalized = raw.replace("\r\n", "\n");
        let trimmed = normalized.trim();

        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.replace('[', "\\[").replace(']', "\\]"))
        }
    }

    /// 1.6j schemas carry the message name in `title`; 2.0.1/2.1 schemas
    /// omit `title` and put it as the last `:`-separated segment of `$id`
    /// instead (e.g. `urn:OCPP:Cp:2:2020:3:BootNotificationRequest`).
    fn message_name(value: &Value) -> anyhow::Result<String> {
        if let Some(title) = value["title"].as_str() {
            return Ok(title.to_string());
        }

        if let Some(id) = value["$id"].as_str() {
            // `rsplit` always yields at least one element, even with no
            // `:` in `id`, so this is infallible.
            let name = id.rsplit(':').next().unwrap();
            return Ok(name.to_string());
        }

        anyhow::bail!("schema has neither a `title` nor a usable `$id`")
    }

    fn action_name(title: &str) -> String {
        title
            .strip_suffix("Request")
            .or_else(|| title.strip_suffix("Response"))
            .unwrap_or(title)
            .to_string()
    }

    /// Field names that carry OCPP protocol semantics get a dedicated
    /// newtype from that version's `primitives` module instead of a plain
    /// bounded string, so the type system distinguishes e.g. an `IdTag`
    /// from an arbitrary string. These are version-specific: `idTag` is a
    /// 1.6J concept with no equivalent shape in 2.0.1/2.1 (which use the
    /// structurally different `IdTokenType`).
    fn wrapper_type_for(version: OcppVersion, field_name: &str) -> Option<&'static str> {
        match (version, field_name) {
            (OcppVersion::V16, "idTag") => Some("IdTag"),
            _ => None,
        }
    }

    fn parse_fields(&mut self, owner: &str, schema: &Value) -> anyhow::Result<Vec<RustField>> {
        let required: HashSet<&str> = schema["required"]
            .as_array()
            .map(|values| values.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();

        let mut fields = Vec::new();

        if let Some(properties) = schema["properties"].as_object() {
            for (name, property) in properties {
                let ty = self.parse_field_type(owner, name, property)?;

                fields.push(RustField {
                    optional: !required.contains(name.as_str()),
                    name: name.to_string(),
                    description: Self::description(property),
                    ty,
                });
            }
        }

        Ok(fields)
    }

    fn parse_field_type(
        &mut self,
        owner: &str,
        field_name: &str,
        schema: &Value,
    ) -> anyhow::Result<RustType> {
        if let Some(wrapper) = Self::wrapper_type_for(self.version, field_name) {
            return Ok(RustType::Primitive(wrapper.to_string()));
        }

        self.resolve_type(owner, field_name, schema)
    }

    fn resolve_type(&mut self, owner: &str, field_name: &str, schema: &Value) -> anyhow::Result<RustType> {
        if let Some(reference) = schema["$ref"].as_str() {
            return self.resolve_ref(reference);
        }

        Ok(match schema["type"].as_str() {
            // An inline (`$ref`-less) enum, e.g. 1.6j's
            // `chargingProfilePurpose`. 2.x always defines enums via `$ref`
            // instead, but 1.6j frequently inlines them, so without this
            // they'd otherwise be treated as plain (unbounded) strings.
            Some("string") if schema["enum"].is_array() => {
                self.resolve_named_type(crate::naming::pascal_case(field_name), schema)?
            }

            // A `heapless::String<N>` needs a capacity. When the spec
            // states one (`maxLength`), use it; otherwise expose a const
            // generic parameter (default `DEFAULT_STRING_CAPACITY`) so the
            // caller picks the bound instead of us guessing one -- or, with
            // the `alloc` feature, this becomes a plain growable `String`.
            Some("string") => match schema["maxLength"].as_u64() {
                Some(max) => RustType::BoundedString(max as usize),
                None => RustType::UnboundedString(ConstParam {
                    name: crate::naming::const_param_name(owner, field_name),
                    default: DEFAULT_STRING_CAPACITY,
                }),
            },

            Some("boolean") => RustType::Bool,

            Some("integer") => RustType::Integer,

            Some("number") => RustType::Number,

            // Same idea as the unbounded-string case above, but for
            // `heapless::Vec<T, N>` / `maxItems`. The item type is resolved
            // under a distinct field-name suffix so an (unlikely) unbounded
            // array of unbounded items gets two independent params instead
            // of accidentally colliding on one.
            Some("array") => {
                let item_field_name = format!("{field_name}Item");
                match schema["maxItems"].as_u64() {
                    Some(max) => {
                        let item_ty = self.resolve_type(owner, &item_field_name, &schema["items"])?;
                        RustType::Vec(Box::new(item_ty), max as usize)
                    }
                    None => {
                        let item_ty = self.resolve_type(owner, &item_field_name, &schema["items"])?;
                        RustType::UnboundedVec(
                            Box::new(item_ty),
                            ConstParam {
                                name: crate::naming::const_param_name(owner, field_name),
                                default: DEFAULT_VEC_CAPACITY,
                            },
                        )
                    }
                }
            }

            // An inline (`$ref`-less) nested object, e.g. 1.6j's
            // `idTagInfo`/`csChargingProfiles`. 2.x always defines these
            // via `$ref` instead, but 1.6j sometimes inlines them. Named by
            // field name alone (not owner-qualified), so the same field
            // name/shape appearing on multiple messages -- `idTagInfo` on
            // both `AuthorizeResponse` and `StartTransactionResponse` --
            // naturally shares one generated type instead of duplicating.
            Some("object") if schema["properties"].is_object() => {
                self.resolve_named_type(crate::naming::pascal_case(field_name), schema)?
            }

            _ => RustType::Unknown,
        })
    }

    /// Resolves a local `#/definitions/Foo` pointer, generating the
    /// referenced type (struct or enum) into the shared pool the first time
    /// it's seen -- by this schema file or an earlier one.
    fn resolve_ref(&mut self, reference: &str) -> anyhow::Result<RustType> {
        let Some(def_name) = reference.strip_prefix("#/definitions/") else {
            return Ok(RustType::Unknown);
        };

        let Some(def_schema) = self.definitions.get(def_name).cloned() else {
            return Ok(RustType::Unknown);
        };

        let rust_name = Self::type_name_for(def_name, &def_schema);

        self.resolve_named_type(rust_name, &def_schema)
    }

    /// Generates a struct or enum into the shared pool under `rust_name`
    /// the first time it's seen -- by an earlier `$ref`, an earlier inline
    /// schema with the same synthesized name, or an earlier schema file
    /// entirely -- returning a reference to it either way. Shared by
    /// `$ref` resolution ([`Self::resolve_ref`]) and inline (`$ref`-less)
    /// object/enum schemas (in [`Self::resolve_type`]).
    fn resolve_named_type(&mut self, rust_name: String, schema: &Value) -> anyhow::Result<RustType> {
        if self.pool.contains(&rust_name) || !self.in_progress.insert(rust_name.clone()) {
            return Ok(RustType::Local(rust_name));
        }

        let generated = if let Some(values) = schema["enum"].as_array() {
            let variants = values
                .iter()
                .filter_map(Value::as_str)
                .map(|raw| RustVariant {
                    ident: crate::naming::rust_variant_ident(raw),
                    raw: raw.to_string(),
                })
                .collect();

            GeneratedType::Enum(RustEnum {
                name: rust_name.clone(),
                description: Self::description(schema),
                variants,
            })
        } else {
            let fields = self.parse_fields(&rust_name, schema)?;

            GeneratedType::Struct(RustStruct {
                name: rust_name.clone(),
                action: None,
                description: Self::description(schema),
                fields,
            })
        };

        self.pool.register(rust_name.clone(), generated)?;

        Ok(RustType::Local(rust_name))
    }

    fn type_name_for(def_name: &str, def_schema: &Value) -> String {
        def_schema["javaType"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| {
                def_name
                    .strip_suffix("Type")
                    .unwrap_or(def_name)
                    .to_string()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn action_name_strips_request_suffix() {
        let schema = json!({
            "title": "AuthorizeRequest",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(parsed.message.action.as_deref(), Some("Authorize"));
    }

    #[test]
    fn action_name_strips_response_suffix() {
        let schema = json!({
            "title": "AuthorizeResponse",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(parsed.message.action.as_deref(), Some("Authorize"));
    }

    #[test]
    fn falls_back_to_id_when_title_is_absent() {
        let schema = json!({
            "$id": "urn:OCPP:Cp:2:2020:3:BootNotificationRequest",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(parsed.message.name, "BootNotificationRequest");
        assert_eq!(parsed.message.action.as_deref(), Some("BootNotification"));
    }

    #[test]
    fn message_level_description_becomes_struct_doc() {
        let schema = json!({
            "title": "AuthorizeRequest",
            "description": "Sent by the Charge Point to the CSMS.",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(
            parsed.message.description.as_deref(),
            Some("Sent by the Charge Point to the CSMS.")
        );
    }

    #[test]
    fn missing_description_is_none() {
        let schema = json!({
            "title": "AuthorizeRequest",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(parsed.message.description, None);
    }

    #[test]
    fn field_level_description_becomes_field_doc() {
        let schema = json!({
            "title": "BootNotificationRequest",
            "type": "object",
            "properties": {
                "reason": {
                    "type": "string",
                    "maxLength": 20,
                    "description": "The reason for sending this message."
                }
            },
            "required": ["reason"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(
            parsed.message.fields[0].description.as_deref(),
            Some("The reason for sending this message.")
        );
    }

    #[test]
    fn crlf_in_description_is_normalized_and_trimmed() {
        let schema = json!({
            "title": "AuthorizeRequest",
            "description": "  Line one.\r\nLine two.\r\n  ",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(
            parsed.message.description.as_deref(),
            Some("Line one.\nLine two.")
        );
    }

    #[test]
    fn square_brackets_in_description_are_escaped_for_rustdoc() {
        // Spec text like `[RFC5646]` (a citation) or `[0-9]` (a character
        // class) is otherwise misread as rustdoc intra-doc link syntax,
        // producing broken-link warnings for links that were never meant
        // to be links.
        let schema = json!({
            "title": "AuthorizeRequest",
            "description": "Language, see [RFC5646] and digits [0-9].",
            "type": "object",
            "properties": {}
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert_eq!(
            parsed.message.description.as_deref(),
            Some("Language, see \\[RFC5646\\] and digits \\[0-9\\].")
        );
    }

    #[test]
    fn nested_struct_and_enum_descriptions_are_captured() {
        let schema = json!({
            "title": "BootNotificationRequest",
            "type": "object",
            "definitions": {
                "ChargingStationType": {
                    "javaType": "ChargingStation",
                    "description": "The physical charging station.",
                    "type": "object",
                    "properties": {},
                    "required": []
                },
                "BootReasonEnumType": {
                    "javaType": "BootReasonEnum",
                    "description": "Reason for sending this message.",
                    "type": "string",
                    "enum": ["PowerUp"]
                }
            },
            "properties": {
                "chargingStation": { "$ref": "#/definitions/ChargingStationType" },
                "reason": { "$ref": "#/definitions/BootReasonEnumType" }
            },
            "required": ["chargingStation", "reason"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        let charging_station = parsed
            .types
            .iter()
            .find_map(|t| match t {
                GeneratedType::Struct(s) if s.name == "ChargingStation" => Some(s),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            charging_station.description.as_deref(),
            Some("The physical charging station.")
        );

        let boot_reason = parsed
            .types
            .iter()
            .find_map(|t| match t {
                GeneratedType::Enum(e) if e.name == "BootReasonEnum" => Some(e),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            boot_reason.description.as_deref(),
            Some("Reason for sending this message.")
        );
    }

    #[test]
    fn id_tag_field_maps_to_wrapper_type() {
        let schema = json!({
            "title": "AuthorizeRequest",
            "type": "object",
            "properties": {
                "idTag": {
                    "type": "string",
                    "maxLength": 20
                }
            },
            "required": ["idTag"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(matches!(&parsed.message.fields[0].ty, RustType::Primitive(name) if name == "IdTag"));
    }

    #[test]
    fn id_tag_wrapper_is_specific_to_v16_and_does_not_apply_to_other_versions() {
        // 2.0.1/2.1 don't have a bare `idTag` string field with this
        // meaning (they use the structurally different `IdTokenType`), so
        // the V16-only wrapper mapping must not accidentally fire for them.
        let schema = json!({
            "title": "AuthorizeRequest",
            "type": "object",
            "properties": {
                "idTag": {
                    "type": "string",
                    "maxLength": 20
                }
            },
            "required": ["idTag"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V201).unwrap();

        assert!(matches!(
            &parsed.message.fields[0].ty,
            RustType::BoundedString(20)
        ));
    }

    #[test]
    fn field_not_in_required_is_optional() {
        let schema = json!({
            "title": "BootNotificationRequest",
            "type": "object",
            "properties": {
                "reason": { "type": "string" }
            },
            "required": []
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(parsed.message.fields[0].optional);
    }

    #[test]
    fn field_in_required_is_not_optional() {
        let schema = json!({
            "title": "BootNotificationRequest",
            "type": "object",
            "properties": {
                "reason": { "type": "string" }
            },
            "required": ["reason"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(!parsed.message.fields[0].optional);
    }

    #[test]
    fn ref_to_object_definition_generates_nested_struct() {
        let schema = json!({
            "title": "BootNotificationRequest",
            "type": "object",
            "definitions": {
                "ChargingStationType": {
                    "javaType": "ChargingStation",
                    "type": "object",
                    "properties": {
                        "model": { "type": "string", "maxLength": 20 },
                        "modem": { "type": "string", "maxLength": 5 }
                    },
                    "required": ["model"]
                }
            },
            "properties": {
                "chargingStation": { "$ref": "#/definitions/ChargingStationType" }
            },
            "required": ["chargingStation"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(
            matches!(&parsed.message.fields[0].ty, RustType::Local(name) if name == "ChargingStation")
        );

        let nested = parsed
            .types
            .iter()
            .find_map(|t| match t {
                GeneratedType::Struct(s) if s.name == "ChargingStation" => Some(s),
                _ => None,
            })
            .expect("nested struct should be generated");

        assert!(nested.action.is_none());
        assert_eq!(nested.fields.len(), 2);
        let model_field = nested.fields.iter().find(|f| f.name == "model").unwrap();
        assert!(!model_field.optional);
        let modem_field = nested.fields.iter().find(|f| f.name == "modem").unwrap();
        assert!(modem_field.optional);
    }

    #[test]
    fn ref_to_enum_definition_generates_enum_with_sanitized_variants() {
        let schema = json!({
            "title": "NotifyEVChargingNeedsRequest",
            "type": "object",
            "definitions": {
                "MeasurandEnumType": {
                    "javaType": "MeasurandEnum",
                    "type": "string",
                    "enum": ["Voltage.Maximum", "Power.Active.Import"]
                }
            },
            "properties": {
                "measurand": { "$ref": "#/definitions/MeasurandEnumType" }
            },
            "required": []
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(
            matches!(&parsed.message.fields[0].ty, RustType::Local(name) if name == "MeasurandEnum")
        );

        let generated_enum = parsed
            .types
            .iter()
            .find_map(|t| match t {
                GeneratedType::Enum(e) if e.name == "MeasurandEnum" => Some(e),
                _ => None,
            })
            .expect("enum should be generated");

        let variant = generated_enum
            .variants
            .iter()
            .find(|v| v.raw == "Voltage.Maximum")
            .unwrap();

        assert_eq!(variant.ident, "VoltageMaximum");
    }

    #[test]
    fn repeated_ref_to_same_definition_is_only_generated_once() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "definitions": {
                "CustomDataType": {
                    "javaType": "CustomData",
                    "type": "object",
                    "properties": {
                        "vendorId": { "type": "string", "maxLength": 255 }
                    },
                    "required": ["vendorId"]
                }
            },
            "properties": {
                "customData": { "$ref": "#/definitions/CustomDataType" },
                "otherData": { "$ref": "#/definitions/CustomDataType" }
            },
            "required": []
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        let count = parsed
            .types
            .iter()
            .filter(|t| matches!(t, GeneratedType::Struct(s) if s.name == "CustomData"))
            .count();

        assert_eq!(count, 1);
    }

    #[test]
    fn definition_name_falls_back_to_stripped_type_suffix_without_java_type() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "definitions": {
                "ModemType": {
                    "type": "object",
                    "properties": {
                        "iccid": { "type": "string", "maxLength": 20 }
                    },
                    "required": []
                }
            },
            "properties": {
                "modem": { "$ref": "#/definitions/ModemType" }
            },
            "required": []
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(matches!(&parsed.message.fields[0].ty, RustType::Local(name) if name == "Modem"));
    }

    #[test]
    fn array_with_max_items_becomes_bounded_vec_of_primitives() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "tags": {
                    "type": "array",
                    "items": { "type": "string", "maxLength": 10 },
                    "maxItems": 5
                }
            },
            "required": ["tags"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::Vec(inner, cap) => {
                assert_eq!(*cap, 5);
                assert!(matches!(**inner, RustType::BoundedString(10)));
            }
            other => panic!("expected Vec, got {other:?}"),
        }
    }

    #[test]
    fn array_without_max_items_becomes_an_unbounded_vec_with_a_named_param() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "tags": {
                    "type": "array",
                    "items": { "type": "string", "maxLength": 10 }
                }
            },
            "required": ["tags"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::UnboundedVec(inner, param) => {
                assert_eq!(param.name, "SOME_REQUEST_TAGS_CAP");
                assert_eq!(param.default, 16);
                assert!(matches!(**inner, RustType::BoundedString(10)));
            }
            other => panic!("expected UnboundedVec, got {other:?}"),
        }
    }

    #[test]
    fn string_without_max_length_becomes_an_unbounded_string_with_a_named_param() {
        // A `heapless::String<N>` needs a capacity, same as `heapless::Vec`;
        // without `maxLength` there's nothing spec-given to size it with,
        // so the caller picks it via a const generic (or gets a plain
        // `alloc::string::String` under the `alloc` feature) instead of us
        // guessing a bound.
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "timestamp": { "type": "string" }
            },
            "required": ["timestamp"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::UnboundedString(param) => {
                assert_eq!(param.name, "SOME_REQUEST_TIMESTAMP_CAP");
                assert_eq!(param.default, 1024);
            }
            other => panic!("expected UnboundedString, got {other:?}"),
        }
    }

    #[test]
    fn unbounded_array_of_unbounded_strings_gets_two_distinct_params() {
        let schema = json!({
            "title": "SomeRequest",
            "type": "object",
            "properties": {
                "notes": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "required": ["notes"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::UnboundedVec(inner, outer_param) => {
                assert_eq!(outer_param.name, "SOME_REQUEST_NOTES_CAP");
                match inner.as_ref() {
                    RustType::UnboundedString(inner_param) => {
                        assert_ne!(inner_param.name, outer_param.name);
                    }
                    other => panic!("expected UnboundedString, got {other:?}"),
                }
            }
            other => panic!("expected UnboundedVec, got {other:?}"),
        }
    }

    #[test]
    fn array_of_ref_items_generates_nested_type_and_local_vec() {
        let schema = json!({
            "title": "MeterValuesRequest",
            "type": "object",
            "definitions": {
                "MeterValueType": {
                    "javaType": "MeterValue",
                    "type": "object",
                    "properties": {
                        "timestamp": { "type": "string" }
                    },
                    "required": ["timestamp"]
                }
            },
            "properties": {
                "meterValue": {
                    "type": "array",
                    "items": { "$ref": "#/definitions/MeterValueType" },
                    "maxItems": 4
                }
            },
            "required": ["meterValue"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        match &parsed.message.fields[0].ty {
            RustType::Vec(inner, cap) => {
                assert_eq!(*cap, 4);
                assert!(matches!(&**inner, RustType::Local(name) if name == "MeterValue"));
            }
            other => panic!("expected Vec, got {other:?}"),
        }

        assert!(parsed
            .types
            .iter()
            .any(|t| matches!(t, GeneratedType::Struct(s) if s.name == "MeterValue")));
    }

    fn schema_with_custom_data(title: &str) -> Value {
        json!({
            "title": title,
            "type": "object",
            "definitions": {
                "CustomDataType": {
                    "javaType": "CustomData",
                    "type": "object",
                    "properties": {
                        "vendorId": { "type": "string", "maxLength": 255 }
                    },
                    "required": ["vendorId"]
                }
            },
            "properties": {
                "customData": { "$ref": "#/definitions/CustomDataType" }
            },
            "required": []
        })
    }

    #[test]
    fn parse_with_pool_shares_a_definition_seen_across_multiple_schema_files() {
        let mut pool = crate::pool::TypePool::new();

        let first = SchemaParser::parse_with_pool(
            &schema_with_custom_data("FirstRequest"),
            &mut pool,
            OcppVersion::V16,
        )
        .unwrap();
        let second = SchemaParser::parse_with_pool(
            &schema_with_custom_data("SecondRequest"),
            &mut pool,
            OcppVersion::V16,
        )
        .unwrap();

        assert!(matches!(&first.fields[0].ty, RustType::Local(name) if name == "CustomData"));
        assert!(matches!(&second.fields[0].ty, RustType::Local(name) if name == "CustomData"));

        let count = pool
            .types()
            .iter()
            .filter(|t| matches!(t, GeneratedType::Struct(s) if s.name == "CustomData"))
            .count();

        assert_eq!(count, 1);
    }

    #[test]
    fn inline_object_without_ref_generates_a_named_local_struct() {
        // Real 1.6j shape: `idTagInfo` on AuthorizeResponse is a nested
        // object defined inline, not via `$ref`/`definitions` (which 2.x
        // always uses instead).
        let schema = json!({
            "title": "AuthorizeResponse",
            "type": "object",
            "properties": {
                "idTagInfo": {
                    "type": "object",
                    "properties": {
                        "status": { "type": "string", "maxLength": 20 }
                    },
                    "required": ["status"]
                }
            },
            "required": ["idTagInfo"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(
            matches!(&parsed.message.fields[0].ty, RustType::Local(name) if name == "IdTagInfo")
        );

        let nested = parsed
            .types
            .iter()
            .find_map(|t| match t {
                GeneratedType::Struct(s) if s.name == "IdTagInfo" => Some(s),
                _ => None,
            })
            .expect("inline object should generate a struct");

        assert_eq!(nested.fields.len(), 1);
        assert_eq!(nested.fields[0].name, "status");
    }

    #[test]
    fn inline_object_with_the_same_field_name_and_shape_is_shared_across_messages() {
        fn schema_with_id_tag_info(title: &str) -> Value {
            json!({
                "title": title,
                "type": "object",
                "properties": {
                    "idTagInfo": {
                        "type": "object",
                        "properties": {
                            "status": { "type": "string", "maxLength": 20 }
                        },
                        "required": ["status"]
                    }
                },
                "required": ["idTagInfo"]
            })
        }

        let mut pool = TypePool::new();

        SchemaParser::parse_with_pool(
            &schema_with_id_tag_info("AuthorizeResponse"),
            &mut pool,
            OcppVersion::V16,
        )
        .unwrap();
        SchemaParser::parse_with_pool(
            &schema_with_id_tag_info("StartTransactionResponse"),
            &mut pool,
            OcppVersion::V16,
        )
        .unwrap();

        let count = pool
            .types()
            .iter()
            .filter(|t| matches!(t, GeneratedType::Struct(s) if s.name == "IdTagInfo"))
            .count();

        assert_eq!(count, 1);
    }

    #[test]
    fn inline_enum_without_ref_generates_a_named_local_enum() {
        let schema = json!({
            "title": "SetChargingProfileRequest",
            "type": "object",
            "properties": {
                "chargingProfilePurpose": {
                    "type": "string",
                    "enum": ["ChargePointMaxProfile", "TxDefaultProfile", "TxProfile"]
                }
            },
            "required": ["chargingProfilePurpose"]
        });

        let parsed = SchemaParser::parse(&schema, OcppVersion::V16).unwrap();

        assert!(
            matches!(&parsed.message.fields[0].ty, RustType::Local(name) if name == "ChargingProfilePurpose")
        );

        let generated_enum = parsed
            .types
            .iter()
            .find_map(|t| match t {
                GeneratedType::Enum(e) if e.name == "ChargingProfilePurpose" => Some(e),
                _ => None,
            })
            .expect("inline enum should generate an enum");

        assert_eq!(generated_enum.variants.len(), 3);
    }
}
