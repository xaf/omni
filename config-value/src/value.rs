use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::extend_strategy::ExtendStrategy;
use crate::primitive::Value;
use crate::scope::Scope;
use crate::source::Source;

/// Configuration data - the recursive data structure
///
/// This represents configuration data where each child node (in mappings/sequences)
/// can have its own source and scope tracking via ConfigValue.
/// Primitive values are stored as Value.
///
/// By default, Value supports Sequence and Mapping, but ConfigData wraps around Value
/// and for Sequence and Mapping directs to another layer of ConfigValue.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigData<S: Source, C: Scope> {
    /// A mapping/object of key-value pairs (each with its own source/scope)
    Mapping(HashMap<String, ConfigValue<S, C>>),
    /// A sequence/array of values (each with its own source/scope)
    Sequence(Vec<ConfigValue<S, C>>),
    /// A primitive value (string, number, boolean, null)
    Value(Value),
}

/// A configuration value with source and scope tracking
///
/// Wraps a ConfigData with metadata about where it came from (source)
/// and what context it applies to (scope).
///
/// Generic over:
/// - `S`: Source type implementing the Source trait
/// - `C`: Scope type implementing the Scope trait
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigValue<S: Source = crate::source::DefaultSource, C: Scope = crate::scope::DefaultScope> {
    source: S,
    scope: C,
    value: Option<Box<ConfigData<S, C>>>,
}

impl<S: Source, C: Scope> ConfigValue<S, C> {
    /// Create a new ConfigValue
    pub fn new(source: S, scope: C, value: Option<Box<ConfigData<S, C>>>) -> Self {
        Self {
            source,
            scope,
            value,
        }
    }

    /// Create a new null ConfigValue with the given source and scope
    pub fn new_null_with(source: S, scope: C) -> Self {
        Self::new(
            source,
            scope,
            Some(Box::new(ConfigData::Value(Value::Null))),
        )
    }

    /// Create a new null ConfigValue with default source and scope
    pub fn new_null() -> Self
    where
        S: Default,
        C: Default,
    {
        Self::new_null_with(S::default(), C::default())
    }

    /// Create an empty ConfigValue (empty mapping) with the given source and scope
    pub fn empty_with(source: S, scope: C) -> Self {
        Self::from_value(
            source,
            scope,
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        )
    }

    /// Create an empty ConfigValue (empty mapping) with default source and scope
    pub fn empty() -> Self
    where
        S: Default,
        C: Default,
    {
        Self::empty_with(S::default(), C::default())
    }

    /// Check if this value is null
    pub fn is_null(&self) -> bool {
        self.value.is_none() || self.as_serde_yaml().is_null()
    }

    /// Create a ConfigValue from a serde_yaml::Value
    pub fn from_value(source: S, scope: C, value: serde_yaml::Value) -> Self {
        let config_data = match value {
            serde_yaml::Value::Mapping(mapping) => {
                ConfigData::Mapping(Self::from_mapping(source.clone(), scope.clone(), mapping))
            }
            serde_yaml::Value::Sequence(sequence) => {
                ConfigData::Sequence(Self::from_sequence(source.clone(), scope.clone(), sequence))
            }
            _ => ConfigData::Value(Value::from(value)),
        };
        Self::new(source, scope, Some(Box::new(config_data)))
    }

    /// Create a ConfigValue from a config_value::Value
    pub fn from_config_value(source: S, scope: C, value: Value) -> Self {
        let config_data = match value {
            Value::Mapping(map) => {
                let mut result = HashMap::new();
                for (k, v) in map {
                    result.insert(k, Self::from_config_value(source.clone(), scope.clone(), v));
                }
                ConfigData::Mapping(result)
            }
            Value::Sequence(seq) => {
                let result = seq
                    .into_iter()
                    .map(|v| Self::from_config_value(source.clone(), scope.clone(), v))
                    .collect();
                ConfigData::Sequence(result)
            }
            _ => ConfigData::Value(value),
        };
        Self::new(source, scope, Some(Box::new(config_data)))
    }

    /// Create a ConfigValue from a YAML string with the given source and scope
    pub fn from_str_with(source: S, scope: C, value: &str) -> Result<Self, serde_yaml::Error> {
        let value = Value::from_yaml_str(value)?;
        Ok(Self::from_config_value(source, scope, value))
    }

    /// Create a ConfigValue from a YAML string with default source and scope
    pub fn from_str(value: &str) -> Result<Self, serde_yaml::Error>
    where
        S: Default,
        C: Default,
    {
        Self::from_str_with(S::default(), C::default(), value)
    }

    /// Create a ConfigValue from a HashMap
    pub fn from_table(source: S, scope: C, table: HashMap<String, ConfigValue<S, C>>) -> Self {
        Self::new(source, scope, Some(Box::new(ConfigData::Mapping(table))))
    }

    fn from_mapping(
        source: S,
        scope: C,
        mapping: serde_yaml::Mapping,
    ) -> HashMap<String, ConfigValue<S, C>> {
        let mut config_mapping = HashMap::new();
        for (key, value) in mapping {
            let key = match key.as_str() {
                Some(key) => key,
                None => continue,
            };

            let new_value = ConfigValue::from_value(source.clone(), scope.clone(), value);
            config_mapping.insert(key.to_string(), new_value);
        }
        config_mapping
    }

    fn from_sequence(
        source: S,
        scope: C,
        sequence: serde_yaml::Sequence,
    ) -> Vec<ConfigValue<S, C>> {
        sequence
            .into_iter()
            .map(|value| ConfigValue::from_value(source.clone(), scope.clone(), value))
            .collect()
    }

    /// Get the source of this value
    pub fn source(&self) -> &S {
        &self.source
    }

    /// Get the scope of this value
    pub fn scope(&self) -> &C {
        &self.scope
    }

    /// Unwrap the ConfigValue to a plain Value
    ///
    /// This recursively strips all source and scope information,
    /// returning only the raw Value structure.
    pub fn unwrap(&self) -> Value {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Value(v) => v.clone(),
                ConfigData::Mapping(mapping) => {
                    let mut result = HashMap::new();
                    for (key, value) in mapping {
                        result.insert(key.clone(), value.unwrap());
                    }
                    Value::Mapping(result)
                }
                ConfigData::Sequence(sequence) => {
                    Value::Sequence(sequence.iter().map(|v| v.unwrap()).collect())
                }
            }
        } else {
            Value::Null
        }
    }

    /// Serialize this value to sorted YAML string
    pub fn as_yaml(&self) -> String {
        let value = self.unwrap();
        let yaml_value: serde_yaml::Value = value.into();
        let sorted = sort_yaml_value(&yaml_value);
        serde_yaml::to_string(&sorted).unwrap_or_default()
    }

    /// Serialize this value to sorted JSON string
    pub fn as_json(&self) -> String {
        let value = self.unwrap();
        let json_value: serde_json::Value = value.into();
        let sorted = sort_json_value(&json_value);
        serde_json::to_string_pretty(&sorted).unwrap_or_default()
    }

    /// Get all scopes present in this value tree
    pub fn scopes(&self) -> HashSet<C> {
        let mut scopes = HashSet::new();
        scopes.insert(self.scope.clone());
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Mapping(mapping) => {
                    for value in mapping.values() {
                        scopes.extend(value.scopes());
                    }
                }
                ConfigData::Sequence(sequence) => {
                    for value in sequence {
                        scopes.extend(value.scopes());
                    }
                }
                _ => {}
            }
        }
        scopes
    }

    /// Get the current (maximum) scope in the value tree
    pub fn current_scope(&self) -> C {
        match self.scopes().iter().max() {
            Some(scope) => scope.clone(),
            None => C::default(),
        }
    }

    /// Filter values to only include a specific scope
    pub fn select_scope(&self, scope: &C) -> Option<ConfigValue<S, C>> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Mapping(mapping) => {
                    let mut new_mapping = HashMap::new();
                    for (key, value) in mapping {
                        if let Some(new_value) = value.select_scope(scope) {
                            new_mapping.insert(key.clone(), new_value);
                        }
                    }
                    if !new_mapping.is_empty() {
                        return Some(ConfigValue {
                            source: self.source.clone(),
                            scope: self.scope.clone(),
                            value: Some(Box::new(ConfigData::Mapping(new_mapping))),
                        });
                    }
                }
                ConfigData::Sequence(sequence) => {
                    let mut new_sequence = Vec::new();
                    for value in sequence {
                        if let Some(new_value) = value.select_scope(scope) {
                            new_sequence.push(new_value);
                        }
                    }
                    if !new_sequence.is_empty() {
                        return Some(ConfigValue {
                            source: self.source.clone(),
                            scope: self.scope.clone(),
                            value: Some(Box::new(ConfigData::Sequence(new_sequence))),
                        });
                    }
                }
                ConfigData::Value(_) => {
                    if self.scope == *scope {
                        return Some(self.clone());
                    }
                }
            }
        }
        None
    }

    /// Filter values to exclude a specific scope
    pub fn reject_scope(&self, scope: &C) -> Option<ConfigValue<S, C>> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Mapping(mapping) => {
                    let mut new_mapping = HashMap::new();
                    for (key, value) in mapping {
                        if let Some(new_value) = value.reject_scope(scope) {
                            new_mapping.insert(key.clone(), new_value);
                        }
                    }
                    if !new_mapping.is_empty() {
                        return Some(ConfigValue {
                            source: self.source.clone(),
                            scope: self.scope.clone(),
                            value: Some(Box::new(ConfigData::Mapping(new_mapping))),
                        });
                    }
                }
                ConfigData::Sequence(sequence) => {
                    let mut new_sequence = Vec::new();
                    for value in sequence {
                        if let Some(new_value) = value.reject_scope(scope) {
                            new_sequence.push(new_value);
                        }
                    }
                    if !new_sequence.is_empty() {
                        return Some(ConfigValue {
                            source: self.source.clone(),
                            scope: self.scope.clone(),
                            value: Some(Box::new(ConfigData::Sequence(new_sequence))),
                        });
                    }
                }
                ConfigData::Value(_) => {
                    if self.scope != *scope {
                        return Some(self.clone());
                    }
                }
            }
        }
        None
    }

    /// Navigate to a value at a specific keypath
    pub fn dig(&self, keypath: Vec<&str>) -> Option<ConfigValue<S, C>> {
        let mut keypath = keypath;
        let key = if !keypath.is_empty() {
            keypath.remove(0)
        } else {
            return Some(self.clone());
        };
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Mapping(mapping) => {
                    if let Some(value) = mapping.get(key) {
                        if keypath.is_empty() {
                            return Some(value.clone());
                        } else {
                            return value.dig(keypath);
                        }
                    }
                }
                ConfigData::Sequence(sequence) => {
                    if let Ok(index) = key.parse::<usize>() {
                        if let Some(value) = sequence.get(index) {
                            if keypath.is_empty() {
                                return Some(value.clone());
                            } else {
                                return value.dig(keypath);
                            }
                        }
                    }
                }
                ConfigData::Value(_) => {}
            }
        }
        None
    }

    /// Navigate to a mutable value at a specific keypath
    pub fn dig_mut(&mut self, keypath: Vec<&str>) -> Option<&mut ConfigValue<S, C>> {
        let mut keypath = keypath;
        let key = if !keypath.is_empty() {
            keypath.remove(0)
        } else {
            return Some(self);
        };

        if let Some(data) = self.value.as_mut().map(|data| data.as_mut()) {
            match data {
                ConfigData::Mapping(mapping) => {
                    if let Some(value) = mapping.get_mut(key) {
                        if keypath.is_empty() {
                            return Some(value);
                        } else {
                            return value.dig_mut(keypath);
                        }
                    }
                }
                ConfigData::Sequence(sequence) => {
                    if let Ok(index) = key.parse::<usize>() {
                        if let Some(value) = sequence.get_mut(index) {
                            if keypath.is_empty() {
                                return Some(value);
                            } else {
                                return value.dig_mut(keypath);
                            }
                        }
                    }
                }
                ConfigData::Value(_) => {}
            }
        }

        None
    }

    /// Get a value by key (single level)
    pub fn get(&self, key: &str) -> Option<ConfigValue<S, C>> {
        match self.dig(vec![key]) {
            Some(config_value) => {
                if let Some(ConfigData::Value(value)) =
                    config_value.value.as_ref().map(|data| data.as_ref())
                {
                    if value.is_null() {
                        return None;
                    }
                }
                Some(config_value)
            }
            None => None,
        }
    }

    /// Get a mutable value by key (single level)
    pub fn get_mut(&mut self, key: &str) -> Option<&mut ConfigValue<S, C>> {
        self.dig_mut(vec![key])
    }

    /// Convert to serde_yaml::Value
    pub fn as_serde_yaml(&self) -> serde_yaml::Value {
        if let Some(ref value) = self.value {
            match **value {
                ConfigData::Mapping(ref mapping) => {
                    let mut serde_mapping = serde_yaml::Mapping::new();
                    for (key, value) in mapping {
                        serde_mapping.insert(
                            serde_yaml::Value::String(key.clone()),
                            value.as_serde_yaml(),
                        );
                    }
                    serde_yaml::Value::Mapping(serde_mapping)
                }
                ConfigData::Sequence(ref sequence) => {
                    let mut serde_sequence = serde_yaml::Sequence::new();
                    for value in sequence {
                        serde_sequence.push(value.as_serde_yaml());
                    }
                    serde_yaml::Value::Sequence(serde_sequence)
                }
                ConfigData::Value(ref value) => value.into(),
            }
        } else {
            serde_yaml::Value::Null
        }
    }

    /// Check if this is a string value
    pub fn is_str(&self) -> bool {
        self.as_str().is_some()
    }

    /// Get as a string
    pub fn as_str(&self) -> Option<String> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            if let Some(value) = value.as_str() {
                return Some(value.to_string());
            }
        }
        None
    }

    /// Check if this is a bool value
    pub fn is_bool(&self) -> bool {
        self.as_bool().is_some()
    }

    /// Get as a bool
    pub fn as_bool(&self) -> Option<bool> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            value.as_bool()
        } else {
            None
        }
    }

    /// Check if this is an integer value
    pub fn is_integer(&self) -> bool {
        self.as_integer().is_some()
    }

    /// Get as an integer
    pub fn as_integer(&self) -> Option<i64> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            value.as_i64()
        } else {
            None
        }
    }

    /// Get as an unsigned integer
    pub fn as_unsigned_integer(&self) -> Option<u64> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            value.as_u64()
        } else {
            None
        }
    }

    /// Check if this is a float value
    pub fn is_float(&self) -> bool {
        self.as_float().is_some()
    }

    /// Get as a float
    pub fn as_float(&self) -> Option<f64> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            value.as_f64()
        } else {
            None
        }
    }

    /// Check if this is an array value
    pub fn is_array(&self) -> bool {
        matches!(
            self.value.as_ref().map(|data| data.as_ref()),
            Some(ConfigData::Sequence(_))
        )
    }

    /// Get as an array
    pub fn as_array(&self) -> Option<Vec<ConfigValue<S, C>>> {
        if let Some(ConfigData::Sequence(sequence)) = self.value.as_ref().map(|data| data.as_ref())
        {
            Some(sequence.clone())
        } else {
            None
        }
    }

    /// Get as a mutable array
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<ConfigValue<S, C>>> {
        if let Some(ConfigData::Sequence(sequence)) = self.value.as_mut().map(|data| data.as_mut())
        {
            Some(sequence)
        } else {
            None
        }
    }

    /// Check if this is a table/mapping value
    pub fn is_table(&self) -> bool {
        matches!(
            self.value.as_ref().map(|data| data.as_ref()),
            Some(ConfigData::Mapping(_))
        )
    }

    /// Get as a table/mapping
    pub fn as_table(&self) -> Option<HashMap<String, ConfigValue<S, C>>> {
        if let Some(ConfigData::Mapping(mapping)) = self.value.as_ref().map(|data| data.as_ref()) {
            Some(mapping.clone())
        } else {
            None
        }
    }

    /// Get as a mutable table/mapping
    pub fn as_table_mut(&mut self) -> Option<&mut HashMap<String, ConfigValue<S, C>>> {
        if let Some(ConfigData::Mapping(mapping)) = self.value.as_mut().map(|data| data.as_mut()) {
            Some(mapping)
        } else {
            None
        }
    }

    /// Get a nested value by key and return it as an array (mutable)
    pub fn get_as_array_mut(&mut self, key: &str) -> Option<&mut Vec<ConfigValue<S, C>>> {
        if let Some(value) = self.get_mut(key) {
            return value.as_array_mut();
        }
        None
    }

    /// Get a nested value by key and return it as a table
    pub fn get_as_table(&self, key: &str) -> Option<HashMap<String, ConfigValue<S, C>>> {
        if let Some(value) = self.get(key) {
            return value.as_table();
        }
        None
    }

    /// Get a nested value by key and return it as a table (mutable)
    pub fn get_as_table_mut(&mut self, key: &str) -> Option<&mut HashMap<String, ConfigValue<S, C>>> {
        if let Some(value) = self.get_mut(key) {
            return value.as_table_mut();
        }
        None
    }

    /// Select specific keys from a mapping and return a new ConfigValue with only those keys
    pub fn select_keys(&self, keys: Vec<String>) -> Option<ConfigValue<S, C>> {
        if let Some(mapping) = self.as_table() {
            let mut new_mapping = HashMap::new();
            for key in keys {
                if let Some(value) = mapping.get(&key) {
                    new_mapping.insert(key, value.clone());
                }
            }
            return Some(ConfigValue::from_table(
                self.source().clone(),
                self.scope().clone(),
                new_mapping,
            ));
        }
        None
    }

    /// Force conversion to string
    ///
    /// Attempts to convert any value to a string representation.
    /// Returns None only for null values or non-primitive types (mappings/sequences).
    pub fn as_str_forced(&self) -> Option<String> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            value.as_str_forced()
        } else {
            None
        }
    }

    /// Force conversion to boolean
    ///
    /// Attempts to convert any value to a boolean.
    /// Supports string conversions like "true", "yes", "1" => true.
    pub fn as_bool_forced(&self) -> Option<bool> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            value.as_bool_forced()
        } else {
            None
        }
    }

    /// Force conversion to integer
    pub fn as_integer_forced(&self) -> Option<i64> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            value.as_i64_forced()
        } else {
            None
        }
    }

    /// Force conversion to float
    pub fn as_float_forced(&self) -> Option<f64> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            value.as_f64_forced()
        } else {
            None
        }
    }

    /// Get a value by key and force to string
    pub fn get_as_str_forced(&self, key: &str) -> Option<String> {
        self.get(key).and_then(|v| v.as_str_forced())
    }

    /// Get a value by key and force to boolean
    pub fn get_as_bool_forced(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| v.as_bool_forced())
    }

    /// Get a value by key and force to integer
    pub fn get_as_integer_forced(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|v| v.as_integer_forced())
    }

    /// Get a value by key and force to unsigned integer
    pub fn get_as_unsigned_integer(&self, key: &str) -> Option<u64> {
        self.get(key).and_then(|v| v.as_unsigned_integer())
    }

    /// Get a value by key and force to float
    pub fn get_as_float_forced(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(|v| v.as_float_forced())
    }

    /// Get a value by key as a string (non-forced, exact type match)
    pub fn get_as_str(&self, key: &str) -> Option<String> {
        self.get(key).and_then(|v| v.as_str())
    }

    /// Get a string array from a key (supports both single values and arrays)
    /// Internal method without error reporting - use get_as_str_array instead
    fn get_as_str_array_internal(&self, key: &str) -> Vec<String> {
        let mut output = Vec::new();

        if let Some(value) = self.get(key) {
            if let Some(s) = value.as_str_forced() {
                output.push(s);
            } else if let Some(array) = value.as_array() {
                for value in array {
                    if let Some(s) = value.as_str_forced() {
                        output.push(s);
                    }
                }
            }
        }

        output
    }

    // Methods with error handler support for validation and reporting

    /// Get a string value or None, reporting type errors via error handler
    pub fn get_as_str_or_none<E: crate::error_handler::ErrorHandler<ErrorKind = crate::ConfigErrorKind>>(
        &self,
        key: &str,
        error_handler: &E,
    ) -> Option<String> {
        if let Some(value) = self.get(key) {
            match value.as_str_forced() {
                Some(value) => Some(value),
                None => {
                    error_handler
                        .clone()
                        .with_expected("string")
                        .with_actual(value.clone())
                        .error(crate::ConfigErrorKind::InvalidValueType);
                    None
                }
            }
        } else {
            None
        }
    }

    /// Get a string value with default, reporting type errors via error handler
    pub fn get_as_str_or_default<E: crate::error_handler::ErrorHandler<ErrorKind = crate::ConfigErrorKind>>(
        &self,
        key: &str,
        default: &str,
        error_handler: &E,
    ) -> String {
        if let Some(value) = self.get(key) {
            match value.as_str_forced() {
                Some(value) => value,
                None => {
                    error_handler
                        .clone()
                        .with_expected("string")
                        .with_actual(value.clone())
                        .error(crate::ConfigErrorKind::InvalidValueType);
                    default.to_string()
                }
            }
        } else {
            default.to_string()
        }
    }

    /// Get a string array, reporting type errors via error handler
    pub fn get_as_str_array<E: crate::error_handler::ErrorHandler<ErrorKind = crate::ConfigErrorKind>>(
        &self,
        key: &str,
        error_handler: &E,
    ) -> Vec<String> {
        let result = self.get_as_str_array_internal(key);

        if result.is_empty() {
            if let Some(value) = self.get(key) {
                if !value.is_array() && !value.is_str() {
                    error_handler
                        .clone()
                        .with_expected("string or array of strings")
                        .with_actual(value.clone())
                        .error(crate::ConfigErrorKind::InvalidValueType);
                }
            }
        }

        result
    }

    /// Get a boolean value or None, reporting type errors via error handler
    pub fn get_as_bool_or_none<E: crate::error_handler::ErrorHandler<ErrorKind = crate::ConfigErrorKind>>(
        &self,
        key: &str,
        error_handler: &E,
    ) -> Option<bool> {
        if let Some(value) = self.get(key) {
            match value.as_bool_forced() {
                Some(value) => Some(value),
                None => {
                    error_handler
                        .clone()
                        .with_expected("bool")
                        .with_actual(value.clone())
                        .error(crate::ConfigErrorKind::InvalidValueType);
                    None
                }
            }
        } else {
            None
        }
    }

    /// Get a boolean value with default, reporting type errors via error handler
    pub fn get_as_bool_or_default<E: crate::error_handler::ErrorHandler<ErrorKind = crate::ConfigErrorKind>>(
        &self,
        key: &str,
        default: bool,
        error_handler: &E,
    ) -> bool {
        if let Some(value) = self.get(key) {
            match value.as_bool_forced() {
                Some(value) => value,
                None => {
                    error_handler
                        .clone()
                        .with_expected("bool")
                        .with_actual(value.clone())
                        .error(crate::ConfigErrorKind::InvalidValueType);
                    default
                }
            }
        } else {
            default
        }
    }

    /// Get a float value or None, reporting type errors via error handler
    pub fn get_as_float_or_none<E: crate::error_handler::ErrorHandler<ErrorKind = crate::ConfigErrorKind>>(
        &self,
        key: &str,
        error_handler: &E,
    ) -> Option<f64> {
        if let Some(value) = self.get(key) {
            match value.as_float() {
                Some(value) => Some(value),
                None => {
                    error_handler
                        .clone()
                        .with_expected("float")
                        .with_actual(value.clone())
                        .error(crate::ConfigErrorKind::InvalidValueType);
                    None
                }
            }
        } else {
            None
        }
    }

    /// Get a float value with default, reporting type errors via error handler
    pub fn get_as_float_or_default<E: crate::error_handler::ErrorHandler<ErrorKind = crate::ConfigErrorKind>>(
        &self,
        key: &str,
        default: f64,
        error_handler: &E,
    ) -> f64 {
        if let Some(value) = self.get(key) {
            match value.as_float() {
                Some(value) => value,
                None => {
                    error_handler
                        .clone()
                        .with_expected("float")
                        .with_actual(value.clone())
                        .error(crate::ConfigErrorKind::InvalidValueType);
                    default
                }
            }
        } else {
            default
        }
    }

    /// Get an integer value or None, reporting type errors via error handler
    pub fn get_as_integer_or_none<E: crate::error_handler::ErrorHandler<ErrorKind = crate::ConfigErrorKind>>(
        &self,
        key: &str,
        error_handler: &E,
    ) -> Option<i64> {
        if let Some(value) = self.get(key) {
            match value.as_integer() {
                Some(value) => Some(value),
                None => {
                    error_handler
                        .clone()
                        .with_expected("integer")
                        .with_actual(value.clone())
                        .error(crate::ConfigErrorKind::InvalidValueType);
                    None
                }
            }
        } else {
            None
        }
    }

    /// Extend this value with another value using merge strategies (internal, recursive)
    pub(crate) fn extend(&mut self, other: ConfigValue<S, C>, extend_strategy: ExtendStrategy) {
        if extend_strategy == ExtendStrategy::Keep && !self.is_none_or_empty() {
            return;
        }

        if let (Some(self_value), Some(other_value)) = (&mut self.value, other.value) {
            match (&mut **self_value, *other_value) {
                (ConfigData::Mapping(self_mapping), ConfigData::Mapping(other_mapping)) => {
                    for (orig_key, value) in other_mapping {
                        let (key, key_strategy) = ExtendStrategy::from_key(&orig_key);
                        let children_strategy = key_strategy.unwrap_or(extend_strategy.clone());

                        if let Some(self_value) = self_mapping.get_mut(&key) {
                            self_value.extend(value, children_strategy);
                        } else {
                            let mut new_value = ConfigValue::new_null_with(other.source.clone(), other.scope.clone());
                            new_value.extend(value, children_strategy);
                            self_mapping.insert(key, new_value);
                        }
                    }
                }
                (ConfigData::Sequence(self_sequence), ConfigData::Sequence(other_sequence)) => {
                    if extend_strategy == ExtendStrategy::Keep && !self_sequence.is_empty() {
                        return;
                    }

                    let _init_index = if extend_strategy == ExtendStrategy::Append {
                        self_sequence.len()
                    } else {
                        0
                    };

                    let mut new_sequence = Vec::new();
                    for (_index, value) in other_sequence.iter().enumerate() {
                        let mut new_value = ConfigValue::new_null_with(other.source.clone(), other.scope.clone());
                        new_value.extend(value.clone(), extend_strategy.clone());
                        new_sequence.push(new_value);
                    }

                    match extend_strategy {
                        ExtendStrategy::Append => {
                            'outer: for new_value in new_sequence {
                                let new_value_yaml = new_value.as_serde_yaml();
                                for old_value in self_sequence.iter() {
                                    let old_value_yaml = old_value.as_serde_yaml();
                                    if old_value_yaml == new_value_yaml {
                                        continue 'outer;
                                    }
                                }
                                self_sequence.push(new_value);
                            }
                        }
                        ExtendStrategy::Prepend => {
                            'outer: for old_value in self_sequence.iter() {
                                let old_value_yaml = old_value.as_serde_yaml();
                                for new_value in new_sequence.iter() {
                                    let new_value_yaml = new_value.as_serde_yaml();
                                    if old_value_yaml == new_value_yaml {
                                        continue 'outer;
                                    }
                                }
                                new_sequence.push(old_value.clone());
                            }
                            *self_sequence = new_sequence;
                        }
                        _ => {
                            *self_sequence = new_sequence;
                        }
                    }
                }
                (ConfigData::Value(self_null), ConfigData::Mapping(other_mapping))
                    if self_null.is_null() || extend_strategy != ExtendStrategy::Keep =>
                {
                    let mut new_mapping = HashMap::new();
                    for (orig_key, value) in other_mapping {
                        let (key, key_strategy) = ExtendStrategy::from_key(&orig_key);
                        let children_strategy = key_strategy.unwrap_or(extend_strategy.clone());

                        let mut new_value = ConfigValue::new_null_with(other.source.clone(), other.scope.clone());
                        new_value.extend(value, children_strategy);
                        new_mapping.insert(key, new_value);
                    }
                    *self_value = Box::new(ConfigData::Mapping(new_mapping));
                }
                (ConfigData::Value(self_null), ConfigData::Sequence(other_sequence))
                    if self_null.is_null() || extend_strategy != ExtendStrategy::Keep =>
                {
                    let mut new_sequence = Vec::new();
                    for (_index, value) in other_sequence.iter().enumerate() {
                        let mut new_value = ConfigValue::new_null_with(other.source.clone(), other.scope.clone());
                        new_value.extend(value.clone(), extend_strategy.clone());
                        new_sequence.push(new_value);
                    }
                    *self_value = Box::new(ConfigData::Sequence(new_sequence));
                }
                (ConfigData::Value(self_null), ConfigData::Value(other_val))
                    if self_null.is_null() || extend_strategy != ExtendStrategy::Keep =>
                {
                    self.source = other.source.clone();
                    self.scope = other.scope.clone();
                    *self_value = Box::new(ConfigData::Value(other_val));
                }
                _ => {}
            }
        }
    }

    fn is_none_or_empty(&self) -> bool {
        self.value.is_none() || self.is_value_empty()
    }

    fn is_value_empty(&self) -> bool {
        if let Some(ref value) = self.value {
            match **value {
                ConfigData::Mapping(ref mapping) => mapping.is_empty(),
                ConfigData::Sequence(ref sequence) => sequence.is_empty(),
                _ => false,
            }
        } else {
            true
        }
    }

    /// Set the value
    pub fn set_value(&mut self, value: Option<Box<ConfigData<S, C>>>) {
        self.value = value;
    }
}

// Implement Serialize for ConfigValue
impl<S: Source, C: Scope> Serialize for ConfigValue<S, C> {
    fn serialize<Se>(&self, serializer: Se) -> Result<Se::Ok, Se::Error>
    where
        Se: serde::Serializer,
    {
        self.as_serde_yaml().serialize(serializer)
    }
}

// Implement Deserialize for ConfigValue
impl<'de, S: Source + Deserialize<'de> + Default, C: Scope + Deserialize<'de>> Deserialize<'de> for ConfigValue<S, C> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        let source = S::default();
        let scope = C::default();
        Ok(ConfigValue::from_value(source, scope, value))
    }
}

// Implement Display for ConfigValue
impl<S: Source, C: Scope> std::fmt::Display for ConfigValue<S, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(&self.as_serde_yaml()).unwrap())
    }
}

// Implement From conversions
impl<S: Source, C: Scope> From<ConfigValue<S, C>> for crate::Value {
    fn from(value: ConfigValue<S, C>) -> Self {
        value.unwrap()
    }
}

impl<S: Source, C: Scope> From<&ConfigValue<S, C>> for crate::Value {
    fn from(value: &ConfigValue<S, C>) -> Self {
        value.unwrap()
    }
}

impl<S: Source, C: Scope> From<ConfigValue<S, C>> for serde_yaml::Value {
    fn from(value: ConfigValue<S, C>) -> Self {
        value.as_serde_yaml()
    }
}

impl<S: Source, C: Scope> From<&ConfigValue<S, C>> for serde_yaml::Value {
    fn from(value: &ConfigValue<S, C>) -> Self {
        value.as_serde_yaml()
    }
}

impl<S: Source + Default, C: Scope + Default> Default for ConfigValue<S, C> {
    fn default() -> Self {
        Self::new_null()
    }
}

/// Sort a YAML value recursively (sorts mapping keys)
pub(crate) fn sort_yaml_value(value: &serde_yaml::Value) -> serde_yaml::Value {
    match value {
        serde_yaml::Value::Mapping(map) => {
            let mut sorted_map = serde_yaml::Mapping::new();
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort_by_key(|k| k.as_str().unwrap_or(""));

            for key in keys {
                if let Some(v) = map.get(key) {
                    sorted_map.insert(key.clone(), sort_yaml_value(v));
                }
            }
            serde_yaml::Value::Mapping(sorted_map)
        }
        serde_yaml::Value::Sequence(seq) => {
            serde_yaml::Value::Sequence(seq.iter().map(sort_yaml_value).collect())
        }
        _ => value.clone(),
    }
}

/// Sort a JSON value recursively (sorts object keys)
pub(crate) fn sort_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(obj) => {
            let mut sorted_obj = serde_json::Map::new();
            let mut keys: Vec<_> = obj.keys().collect();
            keys.sort();

            for key in keys {
                if let Some(v) = obj.get(key) {
                    sorted_obj.insert(key.clone(), sort_json_value(v));
                }
            }
            serde_json::Value::Object(sorted_obj)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(sort_json_value).collect())
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
#[path = "value_test.rs"]
mod tests;
