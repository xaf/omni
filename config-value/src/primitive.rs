use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Primitive configuration value
///
/// Represents configuration values independent of any serialization format (YAML, JSON, TOML, etc.).
/// This is a recursive structure that can represent primitives, sequences, and mappings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// Null/None value
    Null,
    /// Boolean value
    Bool(bool),
    /// Integer value (signed 64-bit)
    Integer(i64),
    /// Unsigned integer value (unsigned 64-bit)
    UnsignedInteger(u64),
    /// Floating point value (64-bit)
    Float(f64),
    /// String value
    String(String),
    /// Sequence/array of values
    Sequence(Vec<Value>),
    /// Mapping/object of key-value pairs
    Mapping(HashMap<String, Value>),
}

impl Value {
    /// Check if this is a null value
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Check if this is a sequence/array
    pub fn is_sequence(&self) -> bool {
        matches!(self, Value::Sequence(_))
    }

    /// Check if this is a mapping/object
    pub fn is_mapping(&self) -> bool {
        matches!(self, Value::Mapping(_))
    }

    /// Try to get as a string
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get as a boolean
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as a sequence
    pub fn as_sequence(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Sequence(seq) => Some(seq),
            _ => None,
        }
    }

    /// Try to get as a mapping
    pub fn as_mapping(&self) -> Option<&HashMap<String, Value>> {
        match self {
            Value::Mapping(map) => Some(map),
            _ => None,
        }
    }

    /// Try to get as an integer
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(*i),
            Value::UnsignedInteger(u) if *u <= i64::MAX as u64 => Some(*u as i64),
            _ => None,
        }
    }

    /// Try to get as an unsigned integer
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::UnsignedInteger(u) => Some(*u),
            Value::Integer(i) if *i >= 0 => Some(*i as u64),
            _ => None,
        }
    }

    /// Try to get as a float
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Integer(i) => Some(*i as f64),
            Value::UnsignedInteger(u) => Some(*u as f64),
            _ => None,
        }
    }

    /// Force conversion to string
    ///
    /// Converts any primitive value to its string representation.
    /// Returns None for Null, Sequence, and Mapping values.
    pub fn as_str_forced(&self) -> Option<String> {
        match self {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            Value::Bool(b) => Some(b.to_string()),
            Value::Integer(i) => Some(i.to_string()),
            Value::UnsignedInteger(u) => Some(u.to_string()),
            Value::Float(f) => Some(f.to_string()),
            Value::Sequence(_) | Value::Mapping(_) => None,
        }
    }

    /// Force conversion to boolean
    ///
    /// Converts various value types to boolean:
    /// - Bool: returns the value
    /// - String: "true", "yes", "y", "on", "1" => true; "false", "no", "n", "off", "0" => false
    /// - Integer/UnsignedInteger: 0 => false, non-zero => true
    /// - Float: 0.0 => false, non-zero => true
    /// - Null, Sequence, Mapping: returns None
    pub fn as_bool_forced(&self) -> Option<bool> {
        match self {
            Value::Null => None,
            Value::Bool(b) => Some(*b),
            Value::String(s) => match s.to_lowercase().as_str() {
                "true" | "yes" | "y" | "on" | "1" => Some(true),
                "false" | "no" | "n" | "off" | "0" => Some(false),
                _ => None,
            },
            Value::Integer(i) => Some(*i != 0),
            Value::UnsignedInteger(u) => Some(*u != 0),
            Value::Float(f) => Some(*f != 0.0),
            Value::Sequence(_) | Value::Mapping(_) => None,
        }
    }

    /// Force conversion to integer
    ///
    /// Converts various value types to i64:
    /// - Integer: returns the value
    /// - UnsignedInteger: returns as i64 if it fits
    /// - Float: truncates to i64
    /// - String: parses as i64
    /// - Bool: true => 1, false => 0
    /// - Null, Sequence, Mapping: returns None
    pub fn as_i64_forced(&self) -> Option<i64> {
        match self {
            Value::Null => None,
            Value::Integer(i) => Some(*i),
            Value::UnsignedInteger(u) if *u <= i64::MAX as u64 => Some(*u as i64),
            Value::UnsignedInteger(_) => None,
            Value::Float(f) => Some(*f as i64),
            Value::String(s) => s.parse().ok(),
            Value::Bool(b) => Some(if *b { 1 } else { 0 }),
            Value::Sequence(_) | Value::Mapping(_) => None,
        }
    }

    /// Force conversion to float
    ///
    /// Converts various value types to f64:
    /// - Float: returns the value
    /// - Integer/UnsignedInteger: converts to f64
    /// - String: parses as f64
    /// - Bool: true => 1.0, false => 0.0
    /// - Null, Sequence, Mapping: returns None
    pub fn as_f64_forced(&self) -> Option<f64> {
        match self {
            Value::Null => None,
            Value::Float(f) => Some(*f),
            Value::Integer(i) => Some(*i as f64),
            Value::UnsignedInteger(u) => Some(*u as f64),
            Value::String(s) => s.parse().ok(),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Value::Sequence(_) | Value::Mapping(_) => None,
        }
    }
}

// Conversions from serde_yaml::Value
impl From<serde_yaml::Value> for Value {
    fn from(value: serde_yaml::Value) -> Self {
        match value {
            serde_yaml::Value::Null => Value::Null,
            serde_yaml::Value::Bool(b) => Value::Bool(b),
            serde_yaml::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Integer(i)
                } else if let Some(u) = n.as_u64() {
                    Value::UnsignedInteger(u)
                } else if let Some(f) = n.as_f64() {
                    Value::Float(f)
                } else {
                    Value::Null
                }
            }
            serde_yaml::Value::String(s) => Value::String(s),
            serde_yaml::Value::Sequence(seq) => {
                Value::Sequence(seq.into_iter().map(Value::from).collect())
            }
            serde_yaml::Value::Mapping(map) => {
                let mut result = HashMap::new();
                for (k, v) in map {
                    if let serde_yaml::Value::String(key) = k {
                        result.insert(key, Value::from(v));
                    }
                }
                Value::Mapping(result)
            }
            serde_yaml::Value::Tagged(tagged) => Value::from(tagged.value),
        }
    }
}

// Conversions to serde_yaml::Value
impl From<Value> for serde_yaml::Value {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => serde_yaml::Value::Null,
            Value::Bool(b) => serde_yaml::Value::Bool(b),
            Value::Integer(i) => serde_yaml::Value::Number(i.into()),
            Value::UnsignedInteger(u) => serde_yaml::Value::Number(u.into()),
            Value::Float(f) => serde_yaml::Value::Number(f.into()),
            Value::String(s) => serde_yaml::Value::String(s),
            Value::Sequence(seq) => {
                serde_yaml::Value::Sequence(seq.into_iter().map(serde_yaml::Value::from).collect())
            }
            Value::Mapping(map) => {
                let mut result = serde_yaml::Mapping::new();
                for (k, v) in map {
                    result.insert(serde_yaml::Value::String(k), serde_yaml::Value::from(v));
                }
                serde_yaml::Value::Mapping(result)
            }
        }
    }
}

impl From<&Value> for serde_yaml::Value {
    fn from(value: &Value) -> Self {
        value.clone().into()
    }
}

// Conversions from serde_json::Value
impl From<serde_json::Value> for Value {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Integer(i)
                } else if let Some(u) = n.as_u64() {
                    Value::UnsignedInteger(u)
                } else if let Some(f) = n.as_f64() {
                    Value::Float(f)
                } else {
                    Value::Null
                }
            }
            serde_json::Value::String(s) => Value::String(s),
            serde_json::Value::Array(arr) => {
                Value::Sequence(arr.into_iter().map(Value::from).collect())
            }
            serde_json::Value::Object(obj) => {
                let mut result = HashMap::new();
                for (k, v) in obj {
                    result.insert(k, Value::from(v));
                }
                Value::Mapping(result)
            }
        }
    }
}

// Conversions to serde_json::Value
impl From<Value> for serde_json::Value {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(b),
            Value::Integer(i) => serde_json::Value::Number(i.into()),
            Value::UnsignedInteger(u) => serde_json::Value::Number(u.into()),
            Value::Float(f) => serde_json::Number::from_f64(f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Value::String(s) => serde_json::Value::String(s),
            Value::Sequence(seq) => {
                serde_json::Value::Array(seq.into_iter().map(serde_json::Value::from).collect())
            }
            Value::Mapping(map) => {
                let mut result = serde_json::Map::new();
                for (k, v) in map {
                    result.insert(k, serde_json::Value::from(v));
                }
                serde_json::Value::Object(result)
            }
        }
    }
}

impl From<&Value> for serde_json::Value {
    fn from(value: &Value) -> Self {
        value.clone().into()
    }
}

// Convenient From implementations for common types
impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Integer(i)
    }
}

impl From<u64> for Value {
    fn from(u: u64) -> Self {
        Value::UnsignedInteger(u)
    }
}

impl From<usize> for Value {
    fn from(u: usize) -> Self {
        Value::UnsignedInteger(u as u64)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}

impl From<Vec<&str>> for Value {
    fn from(v: Vec<&str>) -> Self {
        Value::Sequence(v.into_iter().map(|s| Value::String(s.to_string())).collect())
    }
}

impl From<&[&str]> for Value {
    fn from(v: &[&str]) -> Self {
        Value::Sequence(v.iter().map(|s| Value::String(s.to_string())).collect())
    }
}

impl From<Vec<String>> for Value {
    fn from(v: Vec<String>) -> Self {
        Value::Sequence(v.into_iter().map(Value::String).collect())
    }
}

impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::Sequence(v)
    }
}

impl From<HashMap<String, Value>> for Value {
    fn from(m: HashMap<String, Value>) -> Self {
        Value::Mapping(m)
    }
}

impl Value {
    /// Parse a YAML string into a Value
    ///
    /// This is a convenience method that parses YAML text directly into a Value.
    pub fn from_yaml_str(s: &str) -> Result<Self, serde_yaml::Error> {
        let yaml_value: serde_yaml::Value = serde_yaml::from_str(s)?;
        Ok(Value::from(yaml_value))
    }

    /// Parse a JSON string into a Value
    ///
    /// This is a convenience method that parses JSON text directly into a Value.
    pub fn from_json_str(s: &str) -> Result<Self, serde_json::Error> {
        let json_value: serde_json::Value = serde_json::from_str(s)?;
        Ok(Value::from(json_value))
    }

    /// Serialize this Value to a YAML string
    ///
    /// Returns a YAML representation of the value.
    pub fn to_yaml_string(&self) -> Result<String, serde_yaml::Error> {
        let yaml_value: serde_yaml::Value = self.clone().into();
        serde_yaml::to_string(&yaml_value)
    }

    /// Serialize this Value to a JSON string
    ///
    /// Returns a JSON representation of the value.
    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        let json_value: serde_json::Value = self.clone().into();
        serde_json::to_string(&json_value)
    }

    /// Serialize this Value to a pretty JSON string
    ///
    /// Returns a pretty-printed JSON representation of the value.
    pub fn to_json_string_pretty(&self) -> Result<String, serde_json::Error> {
        let json_value: serde_json::Value = self.clone().into();
        serde_json::to_string_pretty(&json_value)
    }

    /// Convert any serializable type to a Value
    ///
    /// This is similar to serde_yaml::to_value() or serde_json::to_value().
    /// It serializes the input to a YAML value first, then converts to Value.
    pub fn to_value<T: Serialize>(value: T) -> Result<Self, serde_yaml::Error> {
        let yaml_value = serde_yaml::to_value(value)?;
        Ok(Value::from(yaml_value))
    }
}

#[cfg(test)]
#[path = "primitive_test.rs"]
mod tests;
