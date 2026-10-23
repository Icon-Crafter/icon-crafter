//! Design tokens: variable collections, modes and variables.
//!
//! Variables are typed values (colour, number, string, boolean) that can be
//! bound to node properties. Each variable holds one value per mode, enabling
//! light/dark themes. A value can alias another variable; resolution follows
//! the alias chain inside a mode and reports missing/cyclic references instead
//! of guessing.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::ids::{CollectionId, ModeId, VariableId};

/// Value type of a variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VariableType {
    /// A colour.
    #[default]
    Color,
    /// A number.
    Float,
    /// A string.
    String,
    /// A boolean.
    Boolean,
}

/// A variable value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VariableValue {
    /// Colour value.
    Color {
        /// The value.
        color: Color,
    },
    /// Number value.
    Float {
        /// The value.
        value: f64,
    },
    /// String value.
    String {
        /// The value.
        value: String,
    },
    /// Boolean value.
    Boolean {
        /// The value.
        value: bool,
    },
    /// Reference to another variable.
    Alias {
        /// Target variable.
        variable_id: VariableId,
    },
}

impl VariableValue {
    /// Whether this value is an alias.
    pub fn is_alias(&self) -> bool {
        matches!(self, VariableValue::Alias { .. })
    }

    /// The value type described by this value, if not an alias.
    pub fn value_type(&self) -> Option<VariableType> {
        match self {
            VariableValue::Color { .. } => Some(VariableType::Color),
            VariableValue::Float { .. } => Some(VariableType::Float),
            VariableValue::String { .. } => Some(VariableType::String),
            VariableValue::Boolean { .. } => Some(VariableType::Boolean),
            VariableValue::Alias { .. } => None,
        }
    }
}

/// A variable definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variable {
    /// Variable id.
    pub id: VariableId,
    /// Owning collection.
    pub collection_id: CollectionId,
    /// Human readable name, e.g. `color/brand/coral`.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: String,
    /// Declared type.
    #[serde(default)]
    pub resolved_type: VariableType,
    /// Value per mode. Missing modes fall back to the collection's default mode.
    #[serde(default)]
    pub values: IndexMap<ModeId, VariableValue>,
}

impl Variable {
    /// Creates a colour variable with one mode value.
    pub fn color(
        collection_id: CollectionId,
        name: impl Into<String>,
        mode: ModeId,
        color: Color,
    ) -> Variable {
        let mut values = IndexMap::new();
        values.insert(mode, VariableValue::Color { color });
        Variable {
            id: VariableId::new_v4(),
            collection_id,
            name: name.into(),
            description: String::new(),
            resolved_type: VariableType::Color,
            values,
        }
    }

    /// Value in a specific mode, without alias resolution.
    pub fn raw_value(&self, mode: ModeId) -> Option<&VariableValue> {
        self.values.get(&mode)
    }
}

/// A collection of variables sharing a set of modes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableCollection {
    /// Collection id.
    pub id: CollectionId,
    /// Display name.
    pub name: String,
    /// Modes in this collection (at least one).
    #[serde(default)]
    pub modes: Vec<ModeId>,
    /// Default mode.
    pub default_mode: ModeId,
}

impl VariableCollection {
    /// Creates a single-mode collection.
    pub fn new(name: impl Into<String>) -> VariableCollection {
        let mode = ModeId::new_v4();
        VariableCollection {
            id: CollectionId::new_v4(),
            name: name.into(),
            modes: vec![mode],
            default_mode: mode,
        }
    }

    /// Adds a mode and returns its id.
    pub fn add_mode(&mut self) -> ModeId {
        let mode = ModeId::new_v4();
        self.modes.push(mode);
        mode
    }

    /// Whether the collection contains the mode.
    pub fn has_mode(&self, mode: ModeId) -> bool {
        self.modes.contains(&mode)
    }
}

/// All variable collections and variables of a document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VariableRegistry {
    /// Collections.
    #[serde(default)]
    pub collections: Vec<VariableCollection>,
    /// Variables keyed by id.
    #[serde(default)]
    pub variables: IndexMap<VariableId, Variable>,
}

impl VariableRegistry {
    /// Looks up a collection.
    pub fn collection(&self, id: CollectionId) -> Option<&VariableCollection> {
        self.collections
            .iter()
            .find(|collection| collection.id == id)
    }

    /// Looks up a variable.
    pub fn variable(&self, id: VariableId) -> Option<&Variable> {
        self.variables.get(&id)
    }

    /// Number of variables.
    pub fn len(&self) -> usize {
        self.variables.len()
    }

    /// Whether there are no variables.
    pub fn is_empty(&self) -> bool {
        self.variables.is_empty()
    }

    /// Resolves a variable value in a mode, following alias chains.
    ///
    /// Returns `None` when the variable is missing, the value is missing for
    /// both the requested and default mode, the alias chain is broken or it
    /// forms a cycle.
    pub fn resolve(&self, id: VariableId, mode: ModeId) -> Option<VariableValue> {
        let mut current = id;
        let mut guard = 0;
        loop {
            guard += 1;
            if guard > 32 {
                return None;
            }
            let variable = self.variables.get(&current)?;
            let value = self.value_for_mode(variable, mode)?;
            match value {
                VariableValue::Alias { variable_id } => {
                    if *variable_id == current {
                        return None;
                    }
                    current = *variable_id;
                }
                other => return Some(other.clone()),
            }
        }
    }

    fn value_for_mode<'a>(
        &self,
        variable: &'a Variable,
        mode: ModeId,
    ) -> Option<&'a VariableValue> {
        if let Some(value) = variable.values.get(&mode) {
            return Some(value);
        }
        let collection = self.collection(variable.collection_id)?;
        variable.values.get(&collection.default_mode)
    }

    /// Resolved colour value for a colour variable.
    pub fn resolve_color(&self, id: VariableId, mode: ModeId) -> Option<Color> {
        match self.resolve(id, mode)? {
            VariableValue::Color { color } => Some(color),
            _ => None,
        }
    }

    /// Resolved numeric value for a float variable.
    pub fn resolve_float(&self, id: VariableId, mode: ModeId) -> Option<f64> {
        match self.resolve(id, mode)? {
            VariableValue::Float { value } => Some(value),
            _ => None,
        }
    }

    /// Ids of variables that directly alias another variable.
    pub fn aliases(&self) -> impl Iterator<Item = (&VariableId, VariableId)> {
        self.variables.iter().flat_map(|(id, variable)| {
            variable
                .values
                .values()
                .filter_map(move |value| match value {
                    VariableValue::Alias { variable_id } => Some((id, *variable_id)),
                    _ => None,
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with_two_modes() -> (VariableRegistry, CollectionId, ModeId, ModeId) {
        let mut registry = VariableRegistry::default();
        let mut collection = VariableCollection::new("Theme");
        let light = collection.default_mode;
        let dark = collection.add_mode();
        let collection_id = collection.id;
        registry.collections.push(collection);

        let base = Variable::color(
            collection_id,
            "color/background",
            light,
            Color::from_hex("#ffffff").unwrap(),
        );
        let base_id = base.id;
        registry.variables.insert(base_id, base);
        if let Some(variable) = registry.variables.get_mut(&base_id) {
            variable.values.insert(
                dark,
                VariableValue::Color {
                    color: Color::from_hex("#0b0d11").unwrap(),
                },
            );
        }
        (registry, collection_id, light, dark)
    }

    #[test]
    fn resolves_per_mode_with_default_fallback() {
        let (registry, _, light, dark) = registry_with_two_modes();
        let id = *registry.variables.keys().next().unwrap();
        assert_eq!(
            registry.resolve_color(id, light).unwrap().to_hex(),
            "#ffffff"
        );
        assert_eq!(
            registry.resolve_color(id, dark).unwrap().to_hex(),
            "#0b0d11"
        );
        // Unknown mode falls back to the collection default (light).
        assert_eq!(
            registry
                .resolve_color(id, ModeId::new_v4())
                .unwrap()
                .to_hex(),
            "#ffffff"
        );
    }

    #[test]
    fn resolves_alias_chain() {
        let (mut registry, collection_id, light, _) = registry_with_two_modes();
        let target = *registry.variables.keys().next().unwrap();
        let mut alias = Variable {
            id: VariableId::new_v4(),
            collection_id,
            name: "alias".into(),
            description: String::new(),
            resolved_type: VariableType::Color,
            values: IndexMap::new(),
        };
        alias.values.insert(
            light,
            VariableValue::Alias {
                variable_id: target,
            },
        );
        let alias_id = alias.id;
        registry.variables.insert(alias_id, alias);
        assert_eq!(
            registry.resolve_color(alias_id, light).unwrap().to_hex(),
            "#ffffff"
        );
    }

    #[test]
    fn cyclic_alias_does_not_loop_forever() {
        let (mut registry, collection_id, light, _) = registry_with_two_modes();
        let a = VariableId::new_v4();
        let b = VariableId::new_v4();
        for (id, target) in [(a, b), (b, a)] {
            let mut variable = Variable {
                id,
                collection_id,
                name: "cycle".into(),
                description: String::new(),
                resolved_type: VariableType::Color,
                values: IndexMap::new(),
            };
            variable.values.insert(
                light,
                VariableValue::Alias {
                    variable_id: target,
                },
            );
            registry.variables.insert(id, variable);
        }
        assert!(registry.resolve_color(a, light).is_none());
    }
}
