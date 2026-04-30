use crate::actions::{builtin_action_specs, ActionPermission, ConfirmationPolicy, RiskLevel};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::HashMap, fmt};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub risk_level: RiskLevel,
    pub permissions: Vec<ActionPermission>,
    pub confirmation: ConfirmationPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub call_id: String,
    pub ok: bool,
    pub content: Value,
    pub summary_for_model: String,
    pub user_visible_message: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    pub fn builtin() -> Self {
        let mut registry = Self::default();
        for spec in builtin_action_specs() {
            registry.register(ToolDefinition {
                name: spec.name,
                description: spec.description,
                input_schema: spec.input_schema,
                output_schema: spec.output_schema,
                risk_level: spec.risk_level,
                permissions: spec.permissions,
                confirmation: spec.confirmation,
            });
        }
        registry
    }

    pub fn register(&mut self, definition: ToolDefinition) {
        self.tools
            .insert(normalize_tool_name(&definition.name), definition);
    }

    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(&normalize_tool_name(name))
    }

    pub fn definitions(&self) -> Vec<&ToolDefinition> {
        let mut definitions = self.tools.values().collect::<Vec<_>>();
        definitions.sort_by(|left, right| left.name.cmp(&right.name));
        definitions
    }

    pub fn validate_call<'a>(
        &'a self,
        call: &ToolCall,
    ) -> Result<&'a ToolDefinition, ToolValidationError> {
        let definition = self
            .get(&call.name)
            .ok_or_else(|| ToolValidationError::UnknownTool(call.name.clone()))?;
        validate_value_against_schema(&call.arguments, &definition.input_schema, "$")?;
        Ok(definition)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolValidationError {
    UnknownTool(String),
    SchemaViolation(String),
}

impl fmt::Display for ToolValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolValidationError::UnknownTool(name) => write!(formatter, "unknown tool `{name}`"),
            ToolValidationError::SchemaViolation(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for ToolValidationError {}

fn normalize_tool_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn validate_value_against_schema(
    value: &Value,
    schema: &Value,
    path: &str,
) -> Result<(), ToolValidationError> {
    let Some(schema_type) = schema.get("type").and_then(Value::as_str) else {
        return Ok(());
    };

    match schema_type {
        "object" => validate_object(value, schema, path),
        "string" => validate_string(value, schema, path),
        "integer" => validate_integer(value, schema, path),
        "number" => validate_number(value, schema, path),
        "boolean" => validate_boolean(value, path),
        "array" => validate_array(value, schema, path),
        other => Err(ToolValidationError::SchemaViolation(format!(
            "{path}: unsupported schema type `{other}`"
        ))),
    }
}

fn validate_object(value: &Value, schema: &Value, path: &str) -> Result<(), ToolValidationError> {
    let Some(object) = value.as_object() else {
        return Err(ToolValidationError::SchemaViolation(format!(
            "{path}: expected object"
        )));
    };
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    validate_required_properties(object, schema, path)?;

    if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
        for key in object.keys() {
            if !properties.contains_key(key) {
                return Err(ToolValidationError::SchemaViolation(format!(
                    "{path}: unexpected property `{key}`"
                )));
            }
        }
    }

    for (key, item) in object {
        if let Some(property_schema) = properties.get(key) {
            validate_value_against_schema(item, property_schema, &format!("{path}.{key}"))?;
        }
    }

    Ok(())
}

fn validate_required_properties(
    object: &Map<String, Value>,
    schema: &Value,
    path: &str,
) -> Result<(), ToolValidationError> {
    let Some(required) = schema.get("required").and_then(Value::as_array) else {
        return Ok(());
    };

    for field in required.iter().filter_map(Value::as_str) {
        if !object.contains_key(field) {
            return Err(ToolValidationError::SchemaViolation(format!(
                "{path}: missing required property `{field}`"
            )));
        }
    }

    Ok(())
}

fn validate_string(value: &Value, schema: &Value, path: &str) -> Result<(), ToolValidationError> {
    let Some(value) = value.as_str() else {
        return Err(ToolValidationError::SchemaViolation(format!(
            "{path}: expected string"
        )));
    };

    if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
        let matched = allowed
            .iter()
            .filter_map(Value::as_str)
            .any(|item| item == value);
        if !matched {
            return Err(ToolValidationError::SchemaViolation(format!(
                "{path}: value `{value}` is not in enum"
            )));
        }
    }

    Ok(())
}

fn validate_integer(value: &Value, schema: &Value, path: &str) -> Result<(), ToolValidationError> {
    let Some(number) = value
        .as_i64()
        .or_else(|| value.as_u64().map(|value| value as i64))
    else {
        return Err(ToolValidationError::SchemaViolation(format!(
            "{path}: expected integer"
        )));
    };

    validate_numeric_range(number as f64, schema, path)
}

fn validate_number(value: &Value, schema: &Value, path: &str) -> Result<(), ToolValidationError> {
    let Some(number) = value.as_f64() else {
        return Err(ToolValidationError::SchemaViolation(format!(
            "{path}: expected number"
        )));
    };

    validate_numeric_range(number, schema, path)
}

fn validate_numeric_range(
    number: f64,
    schema: &Value,
    path: &str,
) -> Result<(), ToolValidationError> {
    if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64) {
        if number < minimum {
            return Err(ToolValidationError::SchemaViolation(format!(
                "{path}: value is below minimum {minimum}"
            )));
        }
    }
    if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64) {
        if number > maximum {
            return Err(ToolValidationError::SchemaViolation(format!(
                "{path}: value is above maximum {maximum}"
            )));
        }
    }
    Ok(())
}

fn validate_boolean(value: &Value, path: &str) -> Result<(), ToolValidationError> {
    if value.as_bool().is_none() {
        return Err(ToolValidationError::SchemaViolation(format!(
            "{path}: expected boolean"
        )));
    }
    Ok(())
}

fn validate_array(value: &Value, schema: &Value, path: &str) -> Result<(), ToolValidationError> {
    let Some(values) = value.as_array() else {
        return Err(ToolValidationError::SchemaViolation(format!(
            "{path}: expected array"
        )));
    };
    if let Some(item_schema) = schema.get("items") {
        for (index, item) in values.iter().enumerate() {
            validate_value_against_schema(item, item_schema, &format!("{path}[{index}]"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builtin_registry_contains_phase_one_tools() {
        let registry = ToolRegistry::builtin();
        for name in [
            "open_application",
            "open_url",
            "search_web",
            "capture_screen_context",
            "speak_text",
            "set_volume",
            "set_brightness",
            "toggle_vpn",
            "ingest_document",
            "ask_document",
            "summarize_document",
            "read_document_aloud",
            "translate_document",
            "pause_reading",
            "resume_reading",
            "stop_reading",
            "run_safe_command",
        ] {
            assert!(registry.get(name).is_some(), "missing tool {name}");
        }
    }

    #[test]
    fn validates_registered_tool_call_arguments() {
        let registry = ToolRegistry::builtin();
        let call = ToolCall::new(
            "call_1",
            "open_application",
            json!({"app_name": "terminal", "launch_mode": "default"}),
        );

        let definition = registry.validate_call(&call).expect("valid call");
        assert_eq!(definition.name, "open_application");
    }

    #[test]
    fn rejects_unknown_tool() {
        let registry = ToolRegistry::builtin();
        let call = ToolCall::new("call_1", "delete_everything", json!({}));

        assert!(matches!(
            registry.validate_call(&call),
            Err(ToolValidationError::UnknownTool(_))
        ));
    }

    #[test]
    fn rejects_missing_required_property() {
        let registry = ToolRegistry::builtin();
        let call = ToolCall::new("call_1", "open_url", json!({"label": "Example"}));

        assert!(matches!(
            registry.validate_call(&call),
            Err(ToolValidationError::SchemaViolation(_))
        ));
    }

    #[test]
    fn rejects_additional_properties_when_schema_is_strict() {
        let registry = ToolRegistry::builtin();
        let call = ToolCall::new(
            "call_1",
            "open_url",
            json!({"url": "https://example.com", "shell": "rm -rf /"}),
        );

        assert!(matches!(
            registry.validate_call(&call),
            Err(ToolValidationError::SchemaViolation(_))
        ));
    }

    #[test]
    fn rejects_out_of_range_integer() {
        let registry = ToolRegistry::builtin();
        let call = ToolCall::new(
            "call_1",
            "search_web",
            json!({"query": "rust", "max_results": 50}),
        );

        assert!(matches!(
            registry.validate_call(&call),
            Err(ToolValidationError::SchemaViolation(_))
        ));
    }

    #[test]
    fn validates_system_setting_tool_ranges() {
        let registry = ToolRegistry::builtin();
        let call = ToolCall::new("call_1", "set_brightness", json!({"percent": 101}));

        assert!(matches!(
            registry.validate_call(&call),
            Err(ToolValidationError::SchemaViolation(_))
        ));
    }
}
