use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::extend_strategy::ExtendStrategy;
use crate::loader::ExtendOptions;
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

    /// Create a null ConfigValue
    pub fn new_null(source: S, scope: C) -> Self {
        Self::new(
            source,
            scope,
            Some(Box::new(ConfigData::Value(Value::Null))),
        )
    }

    /// Create an empty ConfigValue (empty mapping)
    pub fn empty(source: S, scope: C) -> Self {
        Self::from_value(
            source,
            scope,
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        )
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

    /// Create a ConfigValue from a YAML string
    pub fn from_str(source: S, scope: C, value: &str) -> Result<Self, serde_yaml::Error> {
        let value: serde_yaml::Value = serde_yaml::from_str(value)?;
        Ok(Self::from_value(source, scope, value))
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

    /// Get a value by key and force to float
    pub fn get_as_float_forced(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(|v| v.as_float_forced())
    }

    /// Get a string value with a default fallback
    pub fn get_as_str_or_default(&self, key: &str, default: &str) -> String {
        self.get_as_str_forced(key).unwrap_or_else(|| default.to_string())
    }

    /// Get a boolean value with a default fallback
    pub fn get_as_bool_or_default(&self, key: &str, default: bool) -> bool {
        self.get_as_bool_forced(key).unwrap_or(default)
    }

    /// Get an integer value with a default fallback
    pub fn get_as_integer_or_default(&self, key: &str, default: i64) -> i64 {
        self.get_as_integer_forced(key).unwrap_or(default)
    }

    /// Get a float value with a default fallback
    pub fn get_as_float_or_default(&self, key: &str, default: f64) -> f64 {
        self.get_as_float_forced(key).unwrap_or(default)
    }

    /// Get a string array from a key (supports both single values and arrays)
    pub fn get_as_str_array(&self, key: &str) -> Vec<String> {
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

    /// Extend this value with another value using merge strategies
    pub fn extend(&mut self, other: ConfigValue<S, C>, options: ExtendOptions, keypath: Vec<String>) {
        self.extend_internal(other, options, keypath, None::<&fn(&mut ConfigValue<S, C>, &[String])>);
    }

    /// Extend with custom transform function
    pub fn extend_with_transform<F>(
        &mut self,
        other: ConfigValue<S, C>,
        options: ExtendOptions,
        keypath: Vec<String>,
        transform: F,
    ) where
        F: Fn(&mut ConfigValue<S, C>, &[String]),
    {
        self.extend_internal(other, options, keypath, Some(&transform));
    }

    fn extend_internal<F>(
        &mut self,
        other: ConfigValue<S, C>,
        options: ExtendOptions,
        keypath: Vec<String>,
        transform: Option<&F>,
    ) where
        F: Fn(&mut ConfigValue<S, C>, &[String]),
    {
        if options.strategy == ExtendStrategy::Keep && !self.is_none_or_empty() {
            return;
        }

        if let (Some(self_value), Some(other_value)) = (&mut self.value, other.value) {
            match (&mut **self_value, *other_value) {
                (ConfigData::Mapping(self_mapping), ConfigData::Mapping(other_mapping)) => {
                    for (orig_key, value) in other_mapping {
                        let (key, key_strategy) = ExtendStrategy::from_key(&orig_key);
                        let children_strategy = key_strategy.unwrap_or(options.strategy.clone());

                        let mut keypath = keypath.clone();
                        keypath.push(key.clone());

                        if let Some(self_value) = self_mapping.get_mut(&key) {
                            self_value.extend_internal(
                                value,
                                ExtendOptions::default().with_strategy(children_strategy).with_transform(options.transform),
                                keypath,
                                transform,
                            );
                        } else {
                            let mut new_value = ConfigValue::new_null(other.source.clone(), other.scope.clone());
                            new_value.extend_internal(
                                value,
                                ExtendOptions::default().with_strategy(children_strategy).with_transform(options.transform),
                                keypath,
                                transform,
                            );
                            self_mapping.insert(key, new_value);
                        }
                    }
                }
                (ConfigData::Sequence(self_sequence), ConfigData::Sequence(other_sequence)) => {
                    if options.strategy == ExtendStrategy::Keep && !self_sequence.is_empty() {
                        return;
                    }

                    let init_index = if options.strategy == ExtendStrategy::Append {
                        self_sequence.len()
                    } else {
                        0
                    };

                    let mut new_sequence = Vec::new();
                    for (index, value) in other_sequence.iter().enumerate() {
                        let mut keypath = keypath.clone();
                        keypath.push((init_index + index).to_string());

                        let mut new_value = ConfigValue::new_null(other.source.clone(), other.scope.clone());
                        new_value.extend_internal(
                            value.clone(),
                            options.clone(),
                            keypath,
                            transform,
                        );

                        new_sequence.push(new_value);
                    }

                    match options.strategy {
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
                    if self_null.is_null() || options.strategy != ExtendStrategy::Keep =>
                {
                    let mut new_mapping = HashMap::new();
                    for (orig_key, value) in other_mapping {
                        let (key, key_strategy) = ExtendStrategy::from_key(&orig_key);
                        let children_strategy = key_strategy.unwrap_or(options.strategy.clone());

                        let mut keypath = keypath.clone();
                        keypath.push(key.clone());

                        let mut new_value = ConfigValue::new_null(other.source.clone(), other.scope.clone());
                        new_value.extend_internal(
                            value,
                            ExtendOptions::default().with_strategy(children_strategy).with_transform(options.transform),
                            keypath,
                            transform,
                        );
                        new_mapping.insert(key, new_value);
                    }
                    *self_value = Box::new(ConfigData::Mapping(new_mapping));
                }
                (ConfigData::Value(self_null), ConfigData::Sequence(other_sequence))
                    if self_null.is_null() || options.strategy != ExtendStrategy::Keep =>
                {
                    let mut new_sequence = Vec::new();
                    for (index, value) in other_sequence.iter().enumerate() {
                        let mut keypath = keypath.clone();
                        keypath.push(index.to_string());

                        let mut new_value = ConfigValue::new_null(other.source.clone(), other.scope.clone());
                        new_value.extend_internal(
                            value.clone(),
                            options.clone(),
                            keypath,
                            transform,
                        );

                        new_sequence.push(new_value);
                    }
                    *self_value = Box::new(ConfigData::Sequence(new_sequence));
                }
                (ConfigData::Value(self_null), ConfigData::Value(other_val))
                    if self_null.is_null() || options.strategy != ExtendStrategy::Keep =>
                {
                    self.source = other.source.clone();
                    self.scope = other.scope.clone();
                    *self_value = Box::new(ConfigData::Value(other_val));
                    if options.transform {
                        if let Some(transform_fn) = transform {
                            transform_fn(self, &keypath);
                        }
                    }
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

// Implement Display for ConfigValue
impl<S: Source, C: Scope> std::fmt::Display for ConfigValue<S, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(&self.as_serde_yaml()).unwrap())
    }
}

// Implement From conversions
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

#[cfg(test)]
#[path = "value_test.rs"]
mod tests;
