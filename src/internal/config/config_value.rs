use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::ConfigErrorHandler;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::parser::PathEntryConfig;
use crate::internal::config::utils::sort_serde_yaml;
use crate::internal::env::user_home;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConfigSource {
    Default,
    File(String),
    Package(PathEntryConfig),
    Null,
}

impl Default for ConfigSource {
    fn default() -> Self {
        Self::Default
    }
}

impl ConfigSource {
    pub fn path(&self) -> Option<String> {
        match self {
            Self::File(path) => Some(path.to_string()),
            Self::Package(package) => Some(package.path.clone()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq, Ord, PartialOrd)]
pub enum ConfigScope {
    Null,
    Default,
    System,
    User,
    Workdir,
}

impl Default for ConfigScope {
    fn default() -> Self {
        Self::Default
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConfigData {
    Mapping(HashMap<String, ConfigValue>),
    Sequence(Vec<ConfigValue>),
    Value(serde_yaml::Value),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigExtendOptions {
    strategy: ConfigExtendStrategy,
    transform: bool,
}

impl Default for ConfigExtendOptions {
    fn default() -> Self {
        Self {
            strategy: ConfigExtendStrategy::Default,
            transform: true,
        }
    }
}

impl ConfigExtendOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_strategy(&self, strategy: ConfigExtendStrategy) -> Self {
        Self {
            strategy: strategy.clone(),
            transform: self.transform,
        }
    }

    pub fn with_transform(&self, transform: bool) -> Self {
        Self {
            strategy: self.strategy.clone(),
            transform,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigExtendStrategy {
    Default,
    Append,
    Prepend,
    Replace,
    Keep,
    Raw,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ConfigValue {
    source: ConfigSource,
    scope: ConfigScope,
    value: Option<Box<ConfigData>>,
}

impl AsRef<ConfigData> for ConfigValue {
    fn as_ref(&self) -> &ConfigData {
        self.value
            .as_ref()
            .expect("ConfigValue does not contain a value")
    }
}

impl std::fmt::Display for ConfigValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(&self.unwrap()).unwrap())
    }
}

impl Serialize for ConfigValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_yaml().serialize(serializer)
    }
}

impl Default for ConfigValue {
    fn default() -> Self {
        Self::new(ConfigSource::Null, ConfigScope::Null, None)
    }
}

impl ConfigValue {
    pub fn new(source: ConfigSource, scope: ConfigScope, value: Option<Box<ConfigData>>) -> Self {
        Self {
            source,
            scope,
            value,
        }
    }

    pub fn new_null(source: ConfigSource, scope: ConfigScope) -> Self {
        Self::new(
            source,
            scope,
            Some(Box::new(ConfigData::Value(serde_yaml::Value::Null))),
        )
    }

    pub fn empty() -> Self {
        Self::from_value(
            ConfigSource::Default,
            ConfigScope::Default,
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        )
    }

    pub fn is_null(&self) -> bool {
        self.value.is_none() || self.as_serde_yaml().is_null()
    }

    pub fn from_value(source: ConfigSource, scope: ConfigScope, value: serde_yaml::Value) -> Self {
        let config_data = match value {
            serde_yaml::Value::Mapping(mapping) => {
                ConfigData::Mapping(Self::from_mapping(source.clone(), scope.clone(), mapping))
            }
            serde_yaml::Value::Sequence(sequence) => {
                ConfigData::Sequence(Self::from_sequence(source.clone(), scope.clone(), sequence))
            }
            _ => ConfigData::Value(value),
        };
        Self::new(source, scope.clone(), Some(Box::new(config_data)))
    }

    fn from_mapping(
        source: ConfigSource,
        scope: ConfigScope,
        mapping: serde_yaml::Mapping,
    ) -> HashMap<String, ConfigValue> {
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
        source: ConfigSource,
        scope: ConfigScope,
        sequence: serde_yaml::Sequence,
    ) -> Vec<ConfigValue> {
        let mut config_mapping = Vec::new();
        for value in sequence {
            let new_value = ConfigValue::from_value(source.clone(), scope.clone(), value);
            config_mapping.push(new_value);
        }
        config_mapping
    }

    pub fn from_str(value: &str) -> Result<Self, serde_yaml::Error> {
        let value: serde_yaml::Value = serde_yaml::from_str(value)?;
        Ok(Self::from_value(
            ConfigSource::Null,
            ConfigScope::Null,
            value,
        ))
    }

    pub fn from_table(table: HashMap<String, ConfigValue>) -> Self {
        Self::new(
            ConfigSource::Null,
            ConfigScope::Null,
            Some(Box::new(ConfigData::Mapping(table))),
        )
    }

    pub fn reject_scope(&self, scope: &ConfigScope) -> Option<ConfigValue> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Mapping(mapping) => {
                    let mut new_mapping = HashMap::new();
                    for (key, value) in mapping {
                        if let Some(new_value) = value.reject_scope(scope) {
                            new_mapping.insert(key.to_owned(), new_value);
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

    pub fn select_scope(&self, scope: &ConfigScope) -> Option<ConfigValue> {
        if let Some(data) = self.value.as_ref().map(|data| data.as_ref()) {
            match data {
                ConfigData::Mapping(mapping) => {
                    let mut new_mapping = HashMap::new();
                    for (key, value) in mapping {
                        if let Some(new_value) = value.select_scope(scope) {
                            new_mapping.insert(key.to_owned(), new_value);
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

    pub fn get_scope(&self) -> ConfigScope {
        self.scope.clone()
    }

    pub fn current_scope(&self) -> ConfigScope {
        match self.scopes().iter().max() {
            Some(scope) => scope.clone(),
            None => ConfigScope::Null,
        }
    }

    pub fn scopes(&self) -> HashSet<ConfigScope> {
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

    pub fn dig(&self, keypath: Vec<&str>) -> Option<ConfigValue> {
        let mut keypath = keypath.to_owned();
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

    pub fn dig_mut(&mut self, keypath: Vec<&str>) -> Option<&mut ConfigValue> {
        let mut keypath = keypath.to_owned();
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

    pub fn is_str(&self) -> bool {
        self.as_str().is_some()
    }

    pub fn as_str(&self) -> Option<String> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            if let Some(value) = value.as_str() {
                return Some(value.to_string());
            }
        }
        None
    }

    pub fn as_str_forced(&self) -> Option<String> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            match value {
                serde_yaml::Value::Null => return None,
                serde_yaml::Value::Bool(value) => return Some(value.to_string()),
                serde_yaml::Value::String(value) => return Some(value.to_string()),
                serde_yaml::Value::Number(value) => return Some(value.to_string()),
                serde_yaml::Value::Sequence(_) => return None,
                serde_yaml::Value::Mapping(_) => return None,
                serde_yaml::Value::Tagged(_) => return None,
            }
        }
        None
    }

    pub fn as_str_mut(&mut self) -> Option<&mut String> {
        if let Some(ConfigData::Value(serde_yaml::Value::String(value))) =
            self.value.as_mut().map(|data| data.as_mut())
        {
            return Some(value);
        }
        None
    }

    pub fn is_bool(&self) -> bool {
        self.as_bool().is_some()
    }

    pub fn as_bool(&self) -> Option<bool> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            if let Some(value) = value.as_bool() {
                return Some(value);
            }
        }
        None
    }

    pub fn as_bool_forced(&self) -> Option<bool> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            match value {
                serde_yaml::Value::Null => return None,
                serde_yaml::Value::Bool(value) => return Some(*value),
                serde_yaml::Value::String(value) => match value.to_lowercase().as_str() {
                    "true" | "yes" | "y" | "on" | "1" => return Some(true),
                    "false" | "no" | "n" | "off" | "0" => return Some(false),
                    _ => return None,
                },
                serde_yaml::Value::Number(value) => match value.as_i64() {
                    Some(value) => return Some(value != 0),
                    None => match value.as_f64() {
                        Some(value) => return Some(value != 0.0),
                        None => return None,
                    },
                },
                serde_yaml::Value::Sequence(_) => return None,
                serde_yaml::Value::Mapping(_) => return None,
                serde_yaml::Value::Tagged(_) => return None,
            }
        }
        None
    }

    pub fn is_float(&self) -> bool {
        self.as_float().is_some()
    }

    pub fn as_float(&self) -> Option<f64> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            if let Some(value) = value.as_f64() {
                return Some(value);
            }
        }
        None
    }

    pub fn is_integer(&self) -> bool {
        self.as_integer().is_some()
    }

    pub fn as_integer(&self) -> Option<i64> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            if let Some(value) = value.as_i64() {
                return Some(value);
            }
        }
        None
    }

    pub fn as_unsigned_integer(&self) -> Option<u64> {
        if let Some(ConfigData::Value(value)) = self.value.as_ref().map(|data| data.as_ref()) {
            if let Some(value) = value.as_u64() {
                return Some(value);
            }
        }
        None
    }

    pub fn is_array(&self) -> bool {
        if let Some(ConfigData::Sequence(_)) = self.value.as_ref().map(|data| data.as_ref()) {
            return true;
        }
        false
    }

    pub fn as_array(&self) -> Option<Vec<ConfigValue>> {
        if let Some(ConfigData::Sequence(sequence)) = self.value.as_ref().map(|data| data.as_ref())
        {
            let mut new_sequence = Vec::new();
            for value in sequence {
                new_sequence.push(value.clone());
            }
            return Some(new_sequence);
        }
        None
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<ConfigValue>> {
        if let Some(ConfigData::Sequence(sequence)) = self.value.as_mut().map(|data| data.as_mut())
        {
            return Some(sequence);
        }
        None
    }

    pub fn is_table(&self) -> bool {
        if let Some(ConfigData::Mapping(_)) = self.value.as_ref().map(|data| data.as_ref()) {
            return true;
        }
        false
    }

    pub fn as_table(&self) -> Option<HashMap<String, ConfigValue>> {
        if let Some(ConfigData::Mapping(mapping)) = self.value.as_ref().map(|data| data.as_ref()) {
            let mut new_mapping = HashMap::new();
            for (key, value) in mapping {
                new_mapping.insert(key.to_string(), value.clone());
            }
            return Some(new_mapping);
        }
        None
    }

    pub fn as_table_mut(&mut self) -> Option<&mut HashMap<String, ConfigValue>> {
        if let Some(ConfigData::Mapping(mapping)) = self.value.as_mut().map(|data| data.as_mut()) {
            return Some(mapping);
        }
        None
    }

    pub fn select_keys(&self, keys: Vec<String>) -> Option<ConfigValue> {
        if let Some(ConfigData::Mapping(mapping)) = self.value.as_ref().map(|data| data.as_ref()) {
            let mut new_mapping = HashMap::new();
            for key in keys {
                if let Some(value) = mapping.get(&key) {
                    new_mapping.insert(key, value.clone());
                }
            }
            return Some(ConfigValue {
                value: Some(Box::new(ConfigData::Mapping(new_mapping))),
                scope: self.scope.clone(),
                source: self.source.clone(),
            });
        }
        None
    }

    pub fn get(&self, key: &str) -> Option<ConfigValue> {
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

    pub fn get_mut(&mut self, key: &str) -> Option<&mut ConfigValue> {
        self.dig_mut(vec![key])
    }

    pub fn get_as_str(&self, key: &str) -> Option<String> {
        if let Some(value) = self.get(key) {
            return value.as_str();
        }
        None
    }

    pub fn get_as_str_forced(&self, key: &str) -> Option<String> {
        if let Some(value) = self.get(key) {
            return value.as_str_forced();
        }
        None
    }

    pub fn get_as_str_or_none(
        &self,
        key: &str,
        error_handler: &ConfigErrorHandler,
    ) -> Option<String> {
        if let Some(value) = self.get(key) {
            match value.as_str_forced() {
                Some(value) => Some(value),
                None => {
                    error_handler
                        .with_expected("string")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn get_as_str_or_default(
        &self,
        key: &str,
        default: &str,
        error_handler: &ConfigErrorHandler,
    ) -> String {
        if let Some(value) = self.get(key) {
            match value.as_str_forced() {
                Some(value) => value,
                None => {
                    error_handler
                        .with_expected("string")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    default.to_string()
                }
            }
        } else {
            default.to_string()
        }
    }

    pub fn get_as_str_array(&self, key: &str, error_handler: &ConfigErrorHandler) -> Vec<String> {
        let mut output = Vec::new();

        if let Some(value) = self.get(key) {
            if let Some(value) = value.as_str_forced() {
                output.push(value.to_string());
            } else if let Some(array) = value.as_array() {
                for (idx, value) in array.iter().enumerate() {
                    if let Some(value) = value.as_str_forced() {
                        output.push(value.to_string());
                    } else {
                        error_handler
                            .with_index(idx)
                            .with_expected("string")
                            .with_actual(value)
                            .error(ConfigErrorKind::InvalidValueType);
                    }
                }
            } else {
                error_handler
                    .with_expected("string or array of strings")
                    .with_actual(value)
                    .error(ConfigErrorKind::InvalidValueType);
            }
        }

        output
    }

    pub fn get_as_bool(&self, key: &str) -> Option<bool> {
        if let Some(value) = self.get(key) {
            return value.as_bool();
        }
        None
    }

    pub fn get_as_bool_forced(&self, key: &str) -> Option<bool> {
        if let Some(value) = self.get(key) {
            return value.as_bool_forced();
        }
        None
    }

    pub fn get_as_bool_or_none(
        &self,
        key: &str,
        error_handler: &ConfigErrorHandler,
    ) -> Option<bool> {
        if let Some(value) = self.get(key) {
            match value.as_bool_forced() {
                Some(value) => Some(value),
                None => {
                    error_handler
                        .with_expected("bool")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn get_as_bool_or_default(
        &self,
        key: &str,
        default: bool,
        error_handler: &ConfigErrorHandler,
    ) -> bool {
        if let Some(value) = self.get(key) {
            match value.as_bool_forced() {
                Some(value) => value,
                None => {
                    error_handler
                        .with_expected("bool")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    default
                }
            }
        } else {
            default
        }
    }

    pub fn get_as_float(&self, key: &str) -> Option<f64> {
        if let Some(value) = self.get(key) {
            return value.as_float();
        }
        None
    }

    pub fn get_as_float_or_none(
        &self,
        key: &str,
        error_handler: &ConfigErrorHandler,
    ) -> Option<f64> {
        if let Some(value) = self.get(key) {
            match value.as_float() {
                Some(value) => Some(value),
                None => {
                    error_handler
                        .with_expected("float")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn get_as_float_or_default(
        &self,
        key: &str,
        default: f64,
        error_handler: &ConfigErrorHandler,
    ) -> f64 {
        if let Some(value) = self.get(key) {
            match value.as_float() {
                Some(value) => value,
                None => {
                    error_handler
                        .with_expected("float")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    default
                }
            }
        } else {
            default
        }
    }

    pub fn get_as_integer(&self, key: &str) -> Option<i64> {
        if let Some(value) = self.get(key) {
            return value.as_integer();
        }
        None
    }

    pub fn get_as_integer_or_none(
        &self,
        key: &str,
        error_handler: &ConfigErrorHandler,
    ) -> Option<i64> {
        if let Some(value) = self.get(key) {
            match value.as_integer() {
                Some(value) => Some(value),
                None => {
                    error_handler
                        .with_expected("integer")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn get_as_unsigned_integer(&self, key: &str) -> Option<u64> {
        if let Some(value) = self.get(key) {
            return value.as_unsigned_integer();
        }
        None
    }

    pub fn get_as_array(&self, key: &str) -> Option<Vec<ConfigValue>> {
        if let Some(value) = self.get(key) {
            return value.as_array();
        }
        None
    }

    pub fn get_as_array_mut(&mut self, key: &str) -> Option<&mut Vec<ConfigValue>> {
        if let Some(&mut ref mut value) = self.get_mut(key) {
            return value.as_array_mut();
        }
        None
    }

    pub fn get_as_table(&self, key: &str) -> Option<HashMap<String, ConfigValue>> {
        if let Some(value) = self.get(key) {
            return value.as_table();
        }
        None
    }

    pub fn get_as_table_mut(&mut self, key: &str) -> Option<&mut HashMap<String, ConfigValue>> {
        if let Some(&mut ref mut value) = self.get_mut(key) {
            return value.as_table_mut();
        }
        None
    }

    pub fn unwrap(&self) -> serde_yaml::Value {
        match self.value.as_ref().map(|data| data.as_ref()) {
            Some(ConfigData::Mapping(mapping)) => {
                let mut new_mapping = HashMap::new();
                for (key, value) in mapping {
                    new_mapping.insert(key.to_owned(), value.unwrap().clone());
                }
                serde_yaml::to_value(new_mapping).unwrap()
            }
            Some(ConfigData::Sequence(sequence)) => {
                let mut new_sequence = Vec::new();
                for value in sequence {
                    new_sequence.push(value.unwrap().clone());
                }
                serde_yaml::to_value(new_sequence).unwrap()
            }
            Some(ConfigData::Value(value)) => value.clone(),
            None => serde_yaml::Value::Null,
        }
    }

    pub fn extend(
        &mut self,
        other: ConfigValue,
        options: ConfigExtendOptions,
        keypath: Vec<String>,
    ) {
        if options.strategy == ConfigExtendStrategy::Keep && !self.is_none_or_empty() {
            return;
        }

        if let (Some(self_value), Some(other_value)) = (&mut self.value, other.value) {
            let _cloned_self_value = self_value.clone();
            let _cloned_other_value = other_value.clone();
            match (&mut **self_value, *other_value) {
                (ConfigData::Mapping(self_mapping), ConfigData::Mapping(other_mapping)) => {
                    for (orig_key, value) in other_mapping {
                        let mut key = orig_key.to_owned();
                        let children_strategy =
                            ConfigValue::key_strategy(&mut key, &keypath, &options.strategy);

                        let mut keypath = keypath.clone();
                        keypath.push(key.clone());

                        if let Some(self_value) = self_mapping.get_mut(&key) {
                            self_value.extend(
                                value,
                                options.with_strategy(children_strategy),
                                keypath,
                            );
                        } else {
                            let mut new_value =
                                ConfigValue::new_null(other.source.clone(), other.scope.clone());
                            new_value.extend(
                                value,
                                options.with_strategy(children_strategy),
                                keypath,
                            );
                            self_mapping.insert(key, new_value);
                        }
                    }
                }
                (ConfigData::Sequence(self_sequence), ConfigData::Sequence(other_sequence)) => {
                    if options.strategy == ConfigExtendStrategy::Keep && !self_sequence.is_empty() {
                        return;
                    }

                    let init_index = if options.strategy == ConfigExtendStrategy::Append {
                        self_sequence.len()
                    } else {
                        0
                    };

                    let mut new_sequence = Vec::new();
                    let children_strategy =
                        ConfigValue::key_strategy(&mut "".to_string(), &keypath, &options.strategy);
                    for (index, value) in other_sequence.iter().enumerate() {
                        let mut keypath = keypath.clone();
                        keypath.push((init_index + index).to_string());

                        let mut new_value =
                            ConfigValue::new_null(other.source.clone(), other.scope.clone());
                        new_value.extend(
                            value.clone(),
                            options.with_strategy(children_strategy.clone()),
                            keypath,
                        );

                        new_sequence.push(new_value);
                    }

                    match options.strategy {
                        ConfigExtendStrategy::Append => {
                            'outer: for new_value in new_sequence {
                                let new_value_serde_yaml = new_value.as_serde_yaml();
                                for old_value in self_sequence.iter_mut() {
                                    let old_value_serde_yaml = old_value.as_serde_yaml();
                                    if old_value_serde_yaml == new_value_serde_yaml {
                                        continue 'outer;
                                    }
                                }
                                self_sequence.push(new_value);
                            }
                        }
                        ConfigExtendStrategy::Prepend => {
                            'outer: for old_value in self_sequence.iter_mut() {
                                let old_value_serde_yaml = old_value.as_serde_yaml();
                                for new_value in new_sequence.iter() {
                                    let new_value_serde_yaml = new_value.as_serde_yaml();
                                    if old_value_serde_yaml == new_value_serde_yaml {
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
                    if self_null.is_null() || options.strategy != ConfigExtendStrategy::Keep =>
                {
                    let mut new_mapping = HashMap::new();
                    for (orig_key, value) in other_mapping {
                        let mut key = orig_key.to_owned();
                        let children_strategy =
                            ConfigValue::key_strategy(&mut key, &keypath, &options.strategy);

                        let mut keypath = keypath.clone();
                        keypath.push(key.clone());

                        let mut new_value =
                            ConfigValue::new_null(other.source.clone(), other.scope.clone());
                        new_value.extend(value, options.with_strategy(children_strategy), keypath);
                        new_mapping.insert(key, new_value);
                    }
                    *self_value = Box::new(ConfigData::Mapping(new_mapping));
                }
                (ConfigData::Value(self_null), ConfigData::Sequence(other_sequence))
                    if self_null.is_null() || options.strategy != ConfigExtendStrategy::Keep =>
                {
                    let mut new_sequence = Vec::new();
                    let children_strategy =
                        ConfigValue::key_strategy(&mut "".to_string(), &keypath, &options.strategy);
                    for (index, value) in other_sequence.iter().enumerate() {
                        let mut keypath = keypath.clone();
                        keypath.push(index.to_string());

                        let mut new_value =
                            ConfigValue::new_null(other.source.clone(), other.scope.clone());
                        new_value.extend(
                            value.clone(),
                            options.with_strategy(children_strategy.clone()),
                            keypath,
                        );

                        new_sequence.push(new_value);
                    }
                    *self_value = Box::new(ConfigData::Sequence(new_sequence));
                }
                (ConfigData::Value(self_null), ConfigData::Value(other_val))
                    if self_null.is_null() || options.strategy != ConfigExtendStrategy::Keep =>
                {
                    self.source = other.source.clone();
                    self.scope = other.scope.clone();
                    *self_value = Box::new(ConfigData::Value(other_val));
                    if options.transform {
                        self.transform(&keypath);
                    }
                }
                _ => {
                    // Nothing to do
                }
            }
        } else {
            omni_error!("error parsing configuration files");
        }
    }

    fn key_strategy(
        key: &mut String,
        keypath: &Vec<String>,
        strategy: &ConfigExtendStrategy,
    ) -> ConfigExtendStrategy {
        if *strategy == ConfigExtendStrategy::Raw || (keypath.is_empty() && key == "suggest_config")
        {
            return ConfigExtendStrategy::Raw;
        }

        if *keypath == vec!["path".to_string()] {
            if key == "append" {
                return ConfigExtendStrategy::Append;
            } else if key == "prepend" {
                return ConfigExtendStrategy::Prepend;
            }
        }

        if let Some(real_key) = key.strip_suffix("__toappend") {
            *key = real_key.to_string();
            return ConfigExtendStrategy::Append;
        } else if let Some(real_key) = key.strip_suffix("__toprepend") {
            *key = real_key.to_string();
            return ConfigExtendStrategy::Prepend;
        } else if let Some(real_key) = key.strip_suffix("__toreplace") {
            *key = real_key.to_string();
            return ConfigExtendStrategy::Replace;
        } else if let Some(real_key) = key.strip_suffix("__ifnone") {
            *key = real_key.to_string();
            return ConfigExtendStrategy::Keep;
        }

        ConfigExtendStrategy::Default
    }

    fn keypath_transform(keypath: &[String]) -> bool {
        if keypath.is_empty() {
            return false;
        }

        match (keypath.len(), keypath[0].as_str()) {
            // path => append => <item> or path => prepend => <item>
            (3, "path") => matches!(keypath[1].as_str(), "append" | "prepend"),
            // org => <item> => worktree
            (3, "org") => matches!(keypath[2].as_str(), "worktree"),
            // cache => path
            (2, "cache") => matches!(keypath[1].as_str(), "path"),
            // suggest_clone => template_file
            (2, "suggest_clone") => matches!(keypath[1].as_str(), "template_file"),
            // suggest_config => template_file
            (2, "suggest_config") => matches!(keypath[1].as_str(), "template_file"),
            // worktree
            (1, "worktree") => true,
            _ => false,
        }
    }

    fn transform(&mut self, keypath: &[String]) {
        if !ConfigValue::keypath_transform(keypath) {
            return;
        }

        if let Some(data) = self.value.as_mut().map(|data| data.as_mut()) {
            if let ConfigData::Value(value) = data {
                if let serde_yaml::Value::String(string_value) = value {
                    let value_string = string_value.to_owned();
                    let mut abs_path = value_string.clone();
                    if abs_path.starts_with("~/") {
                        abs_path = Path::new(&user_home())
                            .join(abs_path.trim_start_matches("~/"))
                            .to_str()
                            .unwrap()
                            .to_string();
                    }
                    if !abs_path.starts_with('/') {
                        match self.source.clone() {
                            ConfigSource::File(source) => {
                                if let Some(source) = Path::new(&source).parent() {
                                    abs_path = source.join(abs_path).to_str().unwrap().to_string();
                                }
                            }
                            ConfigSource::Package(source) => {
                                if let Some(relpath) = Path::new(&source.path).parent() {
                                    let relpath =
                                        relpath.join(&abs_path).to_str().unwrap().to_string();

                                    let mut package_path = HashMap::new();
                                    package_path.insert(
                                        "package".to_string(),
                                        ConfigValue {
                                            source: self.source.clone(),
                                            scope: self.scope.clone(),
                                            value: Some(Box::new(ConfigData::Value(
                                                serde_yaml::Value::String(
                                                    source.package.clone().unwrap().to_string(),
                                                ),
                                            ))),
                                        },
                                    );
                                    package_path.insert(
                                        "path".to_string(),
                                        ConfigValue {
                                            source: self.source.clone(),
                                            scope: self.scope.clone(),
                                            value: Some(Box::new(ConfigData::Value(
                                                serde_yaml::Value::String(relpath),
                                            ))),
                                        },
                                    );

                                    *data = ConfigData::Mapping(package_path);
                                    return;
                                }
                            }
                            _ => {}
                        }
                    }
                    *value = serde_yaml::Value::String(abs_path);
                }
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

    pub fn get_source(&self) -> &ConfigSource {
        &self.source
    }

    pub fn as_serde_yaml(&self) -> serde_yaml::Value {
        if let Some(ref value) = self.value {
            match **value {
                ConfigData::Mapping(ref mapping) => {
                    let mut serde_mapping = serde_yaml::Mapping::new();
                    for (key, value) in mapping {
                        serde_mapping.insert(
                            serde_yaml::Value::String(key.to_owned()),
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
                ConfigData::Value(ref value) => value.to_owned(),
            }
        } else {
            serde_yaml::Value::Null
        }
    }

    pub fn as_yaml(&self) -> String {
        let serde_yaml = self.as_serde_yaml();
        let serde_yaml = sort_serde_yaml(&serde_yaml);
        serde_yaml::to_string(&serde_yaml).unwrap()
    }

    pub fn set_value(&mut self, value: Option<Box<ConfigData>>) {
        self.value = value;
    }
}

impl From<ConfigValue> for serde_yaml::Value {
    fn from(value: ConfigValue) -> Self {
        value.as_serde_yaml()
    }
}

impl From<&ConfigValue> for serde_yaml::Value {
    fn from(value: &ConfigValue) -> Self {
        value.as_serde_yaml()
    }
}
