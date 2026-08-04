use crate::model::GeneratedType;
use std::collections::HashMap;

/// Accumulates `$ref`-resolved types across multiple schema files, so a
/// definition shared by several OCPP messages (e.g. `CustomDataType`) is
/// generated once instead of once per message file.
#[derive(Debug, Default)]
pub struct TypePool {
    types: Vec<GeneratedType>,
    by_name: HashMap<String, GeneratedType>,
}

impl TypePool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// Registers `ty` under `name`. If a type with that name was already
    /// registered by an earlier schema file, it must be structurally
    /// identical to what's being registered now -- the same shared OCPP
    /// definition referenced from two messages. Anything else means the
    /// name-mapping heuristic collided two distinct definitions, which is a
    /// bug worth surfacing loudly rather than silently keeping whichever
    /// shape was seen first.
    pub fn register(&mut self, name: String, ty: GeneratedType) -> anyhow::Result<()> {
        if let Some(existing) = self.by_name.get(&name) {
            if existing != &ty {
                anyhow::bail!(
                    "type name collision: `{name}` was generated with two different shapes"
                );
            }
            return Ok(());
        }

        self.by_name.insert(name.clone(), ty.clone());
        self.types.push(ty);
        Ok(())
    }

    pub fn types(&self) -> &[GeneratedType] {
        &self.types
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RustEnum;
    use crate::model::RustStruct;

    fn custom_data() -> GeneratedType {
        GeneratedType::Struct(RustStruct {
            name: "CustomData".to_string(),
            action: None,
            description: None,
            fields: Vec::new(),
        })
    }

    #[test]
    fn registering_the_same_shape_twice_keeps_a_single_entry() {
        let mut pool = TypePool::new();

        pool.register("CustomData".to_string(), custom_data()).unwrap();
        pool.register("CustomData".to_string(), custom_data()).unwrap();

        assert_eq!(pool.types().len(), 1);
    }

    #[test]
    fn registering_different_shapes_under_the_same_name_errors() {
        let mut pool = TypePool::new();

        pool.register("CustomData".to_string(), custom_data())
            .unwrap();

        let different = GeneratedType::Enum(RustEnum {
            name: "CustomData".to_string(),
            description: None,
            variants: Vec::new(),
        });

        assert!(pool.register("CustomData".to_string(), different).is_err());
    }

    #[test]
    fn contains_reflects_registered_names() {
        let mut pool = TypePool::new();

        assert!(!pool.contains("CustomData"));

        pool.register("CustomData".to_string(), custom_data()).unwrap();

        assert!(pool.contains("CustomData"));
    }
}
