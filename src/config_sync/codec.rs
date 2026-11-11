//! Format codecs: a config file's text as a document the sync can read and edit, and back.
//!
//! A codec knows nothing about who owns which part of a config. It parses text into a
//! [`ConfigTree`], answers questions about paths and values in it, edits it, and renders it again;
//! the engine decides what to edit.

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value as JsonValue};
use serde_norway::{Mapping, Value as YamlValue};
use toml_edit::{Array, DocumentMut, Item, TableLike};

use crate::provider::ownership::ConfigFormat;

/// Which side of a sync a document is parsed for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// The rendered config, which must say something.
    Generated,
    /// The user's config, where an empty YAML file is an empty mapping.
    Installed,
}

/// A parsed config whose top level is a mapping.
pub(crate) trait ConfigTree: Sized {
    type Value: Clone;

    /// # Errors
    ///
    /// Returns an error when `text` is not a document of this format with a mapping at its root.
    fn parse(text: &str, role: Role) -> Result<Self>;

    fn blank() -> Self;

    /// # Errors
    ///
    /// Returns an error when the document cannot be serialized.
    fn render(&self) -> Result<String>;

    /// The entries of the root mapping, in document order.
    fn entries(&self) -> Vec<(String, Self::Value)>;

    /// The value at `path`, or `None` when any step is missing or is not a mapping.
    fn get(&self, path: &[&str]) -> Option<Self::Value>;

    /// Sets the value at `path`, creating the mappings on the way.
    ///
    /// # Errors
    ///
    /// Returns an error when a step on the way exists and is not a mapping.
    fn set(&mut self, path: &[&str], value: Self::Value) -> Result<()>;

    /// Removes the value at `path`; a missing path has nothing to remove.
    fn remove(&mut self, path: &[&str]);

    fn mapping(value: &Self::Value) -> Option<Vec<(String, Self::Value)>>;
    fn sequence(value: &Self::Value) -> Option<Vec<Self::Value>>;
    fn from_sequence(items: Vec<Self::Value>) -> Self::Value;
    fn text(value: &Self::Value) -> Option<String>;
    fn from_text(text: String) -> Self::Value;
    fn same(left: &Self::Value, right: &Self::Value) -> bool;
}

/// The parent of the last step of `path`, and that last step.
fn split_path<'a, 'b>(path: &'a [&'b str]) -> Result<(&'a [&'b str], &'b str)> {
    path.split_last()
        .map(|(last, parents)| (parents, *last))
        .context("a config path needs at least one step")
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Json(JsonValue);

impl Json {
    pub(crate) fn from_value(value: JsonValue) -> Result<Self> {
        match value {
            JsonValue::Object(_) => Ok(Self(value)),
            _ => bail!("the root must be a JSON object"),
        }
    }
}

fn json_parent_mut<'a>(
    root: &'a mut JsonValue,
    parents: &[&str],
) -> Result<&'a mut Map<String, JsonValue>> {
    let mut object = root
        .as_object_mut()
        .context("the root must be a JSON object")?;
    for key in parents {
        object = object
            .entry(*key)
            .or_insert_with(|| JsonValue::Object(Map::new()))
            .as_object_mut()
            .with_context(|| format!("{key} must be a JSON object"))?;
    }
    Ok(object)
}

fn json_set(root: &mut JsonValue, path: &[&str], value: JsonValue) -> Result<()> {
    let (parents, last) = split_path(path)?;
    json_parent_mut(root, parents)?.insert(last.to_owned(), value);
    Ok(())
}

fn json_remove(root: &mut JsonValue, path: &[&str]) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let parent = parents
        .iter()
        .try_fold(root, |value, key| value.get_mut(*key));
    if let Some(object) = parent.and_then(JsonValue::as_object_mut) {
        object.remove(*last);
    }
}

fn json_entries(value: &JsonValue) -> Option<Vec<(String, JsonValue)>> {
    let object = value.as_object()?;
    Some(object.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

impl ConfigTree for Json {
    type Value = JsonValue;

    fn parse(text: &str, _role: Role) -> Result<Self> {
        Self::from_value(serde_json::from_str(text).context("invalid JSON")?)
    }

    fn blank() -> Self {
        Self(JsonValue::Object(Map::new()))
    }

    fn render(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(&self.0)? + "\n")
    }

    fn entries(&self) -> Vec<(String, JsonValue)> {
        json_entries(&self.0).unwrap_or_default()
    }

    fn get(&self, path: &[&str]) -> Option<JsonValue> {
        path.iter()
            .try_fold(&self.0, |value, key| value.get(*key))
            .cloned()
    }

    fn set(&mut self, path: &[&str], value: JsonValue) -> Result<()> {
        json_set(&mut self.0, path, value)
    }

    fn remove(&mut self, path: &[&str]) {
        json_remove(&mut self.0, path);
    }

    fn mapping(value: &JsonValue) -> Option<Vec<(String, JsonValue)>> {
        json_entries(value)
    }

    fn sequence(value: &JsonValue) -> Option<Vec<JsonValue>> {
        value.as_array().cloned()
    }

    fn from_sequence(items: Vec<JsonValue>) -> JsonValue {
        JsonValue::Array(items)
    }

    fn text(value: &JsonValue) -> Option<String> {
        value.as_str().map(ToOwned::to_owned)
    }

    fn from_text(text: String) -> JsonValue {
        JsonValue::String(text)
    }

    fn same(left: &JsonValue, right: &JsonValue) -> bool {
        left == right
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Yaml(Mapping);

fn yaml_parent_mut<'a>(root: &'a mut Mapping, parents: &[&str]) -> Result<&'a mut Mapping> {
    let mut mapping = root;
    for key in parents {
        mapping = mapping
            .entry(YamlValue::String((*key).to_owned()))
            .or_insert_with(|| YamlValue::Mapping(Mapping::new()))
            .as_mapping_mut()
            .with_context(|| format!("{key} must be a mapping"))?;
    }
    Ok(mapping)
}

fn yaml_set(root: &mut Mapping, path: &[&str], value: YamlValue) -> Result<()> {
    let (parents, last) = split_path(path)?;
    yaml_parent_mut(root, parents)?.insert(YamlValue::String(last.to_owned()), value);
    Ok(())
}

fn yaml_remove(root: &mut Mapping, path: &[&str]) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut mapping = Some(root);
    for key in parents {
        mapping = mapping
            .and_then(|mapping| mapping.get_mut(*key))
            .and_then(YamlValue::as_mapping_mut);
    }
    if let Some(mapping) = mapping {
        mapping.remove(*last);
    }
}

fn yaml_entries(mapping: &Mapping) -> Vec<(String, YamlValue)> {
    mapping
        .iter()
        .filter_map(|(key, value)| Some((key.as_str()?.to_owned(), value.clone())))
        .collect()
}

impl ConfigTree for Yaml {
    type Value = YamlValue;

    fn parse(text: &str, role: Role) -> Result<Self> {
        match serde_norway::from_str(text).context("invalid YAML")? {
            YamlValue::Mapping(mapping) => Ok(Self(mapping)),
            YamlValue::Null if role == Role::Installed => Ok(Self::blank()),
            _ => bail!("the root must be a YAML mapping"),
        }
    }

    fn blank() -> Self {
        Self(Mapping::new())
    }

    fn render(&self) -> Result<String> {
        Ok(serde_norway::to_string(&self.0)?)
    }

    fn entries(&self) -> Vec<(String, YamlValue)> {
        yaml_entries(&self.0)
    }

    fn get(&self, path: &[&str]) -> Option<YamlValue> {
        let (first, rest) = path.split_first()?;
        let first = self.0.get(*first)?;
        rest.iter()
            .try_fold(first, |value, key| value.get(*key))
            .cloned()
    }

    fn set(&mut self, path: &[&str], value: YamlValue) -> Result<()> {
        yaml_set(&mut self.0, path, value)
    }

    fn remove(&mut self, path: &[&str]) {
        yaml_remove(&mut self.0, path);
    }

    fn mapping(value: &YamlValue) -> Option<Vec<(String, YamlValue)>> {
        value.as_mapping().map(yaml_entries)
    }

    fn sequence(value: &YamlValue) -> Option<Vec<YamlValue>> {
        value.as_sequence().cloned()
    }

    fn from_sequence(items: Vec<YamlValue>) -> YamlValue {
        YamlValue::Sequence(items)
    }

    fn text(value: &YamlValue) -> Option<String> {
        value.as_str().map(ToOwned::to_owned)
    }

    fn from_text(text: String) -> YamlValue {
        YamlValue::String(text)
    }

    fn same(left: &YamlValue, right: &YamlValue) -> bool {
        left == right
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Toml(DocumentMut);

fn toml_parent_mut<'a>(
    root: &'a mut DocumentMut,
    parents: &[&str],
) -> Result<&'a mut dyn TableLike> {
    let mut table: &mut dyn TableLike = root.as_table_mut();
    for key in parents {
        table = table
            .entry(key)
            .or_insert_with(toml_edit::table)
            .as_table_like_mut()
            .with_context(|| format!("{key} is not a table"))?;
    }
    Ok(table)
}

fn toml_set(root: &mut DocumentMut, path: &[&str], value: Item) -> Result<()> {
    let (parents, last) = split_path(path)?;
    toml_parent_mut(root, parents)?.insert(last, value);
    Ok(())
}

fn toml_remove(root: &mut DocumentMut, path: &[&str]) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut table: Option<&mut dyn TableLike> = Some(root.as_table_mut());
    for key in parents {
        table = table
            .and_then(|table| table.get_mut(key))
            .and_then(Item::as_table_like_mut);
    }
    if let Some(table) = table {
        table.remove(last);
    }
}

fn toml_entries(table: &dyn TableLike) -> Vec<(String, Item)> {
    table
        .iter()
        .map(|(key, item)| (key.to_owned(), item.clone()))
        .collect()
}

impl ConfigTree for Toml {
    type Value = Item;

    fn parse(text: &str, _role: Role) -> Result<Self> {
        Ok(Self(text.parse::<DocumentMut>().context("invalid TOML")?))
    }

    fn blank() -> Self {
        Self(DocumentMut::new())
    }

    fn render(&self) -> Result<String> {
        Ok(self.0.to_string())
    }

    fn entries(&self) -> Vec<(String, Item)> {
        toml_entries(self.0.as_table())
    }

    fn get(&self, path: &[&str]) -> Option<Item> {
        let (first, rest) = path.split_first()?;
        let first = self.0.get(first)?;
        rest.iter()
            .try_fold(first, |item, key| item.get(*key))
            .cloned()
    }

    fn set(&mut self, path: &[&str], value: Item) -> Result<()> {
        toml_set(&mut self.0, path, value)
    }

    fn remove(&mut self, path: &[&str]) {
        toml_remove(&mut self.0, path);
    }

    fn mapping(value: &Item) -> Option<Vec<(String, Item)>> {
        value.as_table_like().map(toml_entries)
    }

    fn sequence(value: &Item) -> Option<Vec<Item>> {
        value
            .as_array()
            .map(|array| array.iter().cloned().map(Item::Value).collect())
    }

    fn from_sequence(items: Vec<Item>) -> Item {
        let array: Array = items
            .into_iter()
            .filter_map(|item| item.into_value().ok())
            .collect();
        Item::Value(array.into())
    }

    fn text(value: &Item) -> Option<String> {
        value.as_str().map(ToOwned::to_owned)
    }

    fn from_text(text: String) -> Item {
        toml_edit::value(text)
    }

    fn same(left: &Item, right: &Item) -> bool {
        left.to_string() == right.to_string()
    }
}

/// A config of any format, parsed once.
#[derive(Clone, Debug)]
pub(crate) enum ConfigDocument {
    Json(Json),
    Toml(Toml),
    Yaml(Yaml),
}

impl ConfigDocument {
    /// # Errors
    ///
    /// Returns an error when `text` is not a document of `format`.
    pub(crate) fn parse(format: ConfigFormat, text: &str, role: Role) -> Result<Self> {
        Ok(match format {
            ConfigFormat::Json => Self::Json(Json::parse(text, role)?),
            ConfigFormat::Toml => Self::Toml(Toml::parse(text, role)?),
            ConfigFormat::Yaml => Self::Yaml(Yaml::parse(text, role)?),
        })
    }

    pub(crate) fn blank(format: ConfigFormat) -> Self {
        match format {
            ConfigFormat::Json => Self::Json(Json::blank()),
            ConfigFormat::Toml => Self::Toml(Toml::blank()),
            ConfigFormat::Yaml => Self::Yaml(Yaml::blank()),
        }
    }

    pub(crate) fn format(&self) -> ConfigFormat {
        match self {
            Self::Json(_) => ConfigFormat::Json,
            Self::Toml(_) => ConfigFormat::Toml,
            Self::Yaml(_) => ConfigFormat::Yaml,
        }
    }
}

#[cfg(test)]
mod tests;
