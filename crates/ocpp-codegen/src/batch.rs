use crate::model::OcppVersion;
use crate::pool::TypePool;
use crate::schema::SchemaParser;
use serde_json::Value;

pub struct GeneratedMessage {
    /// The message struct's Rust name (e.g. `BootNotificationRequest`),
    /// useful for deriving an output file name.
    pub struct_name: String,
    pub source: String,
}

pub struct BatchOutput {
    /// Contents of the shared `common.rs`, holding every type reachable via
    /// `$ref` from any of the input schemas, deduplicated by name.
    pub common: String,
    pub messages: Vec<GeneratedMessage>,
}

/// Generates many schema files together, sharing one [`TypePool`] so a
/// definition referenced by more than one message (e.g. `CustomDataType`)
/// is generated once into `common.rs` instead of once per message file.
pub fn generate_batch(schemas: &[Value], version: OcppVersion) -> anyhow::Result<BatchOutput> {
    let mut pool = TypePool::new();
    let mut parsed_messages = Vec::with_capacity(schemas.len());

    // Cross-file collisions (1.6j's `status`/`type`/`unit` meaning
    // different things on different messages) can only be detected by
    // looking at every schema in the batch up front -- see
    // `find_ambiguous_inline_enum_names`.
    let ambiguous_inline_enum_names = crate::schema::find_ambiguous_inline_enum_names(schemas);

    for schema in schemas {
        parsed_messages.push(SchemaParser::parse_with_pool_ambiguous(
            schema,
            &mut pool,
            version,
            &ambiguous_inline_enum_names,
        )?);
    }

    // Computed only after every schema file has registered its types, since
    // a message's `Eq`-ness or generic params can depend on a shared type
    // that a *different* schema file was the one to register.
    let eq_by_name = crate::eqcheck::compute_eq_derivable(pool.types());
    let const_params = crate::generics::compute_const_params(pool.types());

    let messages = parsed_messages
        .into_iter()
        .map(|message| GeneratedMessage {
            struct_name: message.name.clone(),
            source: crate::rust::generate_message(&message, &eq_by_name, &const_params),
        })
        .collect();

    let common = crate::rust::generate_common(pool.types());

    Ok(BatchOutput { common, messages })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn shared_definition_is_generated_once_in_common_not_per_message() {
        let schemas = vec![
            schema_with_custom_data("FirstRequest"),
            schema_with_custom_data("SecondRequest"),
        ];

        let output = generate_batch(&schemas, OcppVersion::V16).unwrap();

        let common_occurrences = output.common.matches("pub struct CustomData").count();
        assert_eq!(common_occurrences, 1);

        assert_eq!(output.messages.len(), 2);
        for message in &output.messages {
            assert!(!message.source.contains("pub struct CustomData"));
            assert!(message.source.contains("use super::common::*"));
            assert!(message.source.contains("CustomData"));
        }
    }

    #[test]
    fn eq_derivability_accounts_for_types_registered_by_other_schema_files() {
        // File 1 registers `Sample` (has a float field, so it's not `Eq`)
        // into the shared pool -- nothing in file 2 mentions this directly.
        let sample_holder = json!({
            "title": "SampleHolderRequest",
            "type": "object",
            "definitions": {
                "SampleType": {
                    "javaType": "Sample",
                    "type": "object",
                    "properties": {
                        "value": { "type": "number" }
                    },
                    "required": ["value"]
                }
            },
            "properties": {
                "sample": { "$ref": "#/definitions/SampleType" }
            },
            "required": ["sample"]
        });

        // File 2 nests `Sample` (via an identical local definition, the
        // same OCPP type reused across schema files) inside `Wrapper`, and
        // its own message only references `Wrapper`, never `Sample`
        // directly.
        let wrapper_holder = json!({
            "title": "WrapperHolderRequest",
            "type": "object",
            "definitions": {
                "SampleType": {
                    "javaType": "Sample",
                    "type": "object",
                    "properties": {
                        "value": { "type": "number" }
                    },
                    "required": ["value"]
                },
                "WrapperType": {
                    "javaType": "Wrapper",
                    "type": "object",
                    "properties": {
                        "sample": { "$ref": "#/definitions/SampleType" }
                    },
                    "required": ["sample"]
                }
            },
            "properties": {
                "wrapper": { "$ref": "#/definitions/WrapperType" }
            },
            "required": ["wrapper"]
        });

        let output = generate_batch(&[sample_holder, wrapper_holder], OcppVersion::V16).unwrap();

        let wrapper_holder_source = &output
            .messages
            .iter()
            .find(|m| m.struct_name == "WrapperHolderRequest")
            .unwrap()
            .source;

        assert!(!wrapper_holder_source.contains("PartialEq, Eq"));
    }

    #[test]
    fn message_struct_names_are_reported_for_file_naming() {
        let schemas = vec![schema_with_custom_data("FirstRequest")];

        let output = generate_batch(&schemas, OcppVersion::V16).unwrap();

        assert_eq!(output.messages[0].struct_name, "FirstRequest");
    }
}
