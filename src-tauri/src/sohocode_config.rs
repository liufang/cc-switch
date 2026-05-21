use crate::config::write_json_file;
use crate::error::AppError;
use crate::settings::get_sohocode_override_dir;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub fn get_sohocode_dir() -> PathBuf {
    if let Some(override_dir) = get_sohocode_override_dir() {
        return override_dir;
    }

    crate::config::get_home_dir().join(".sohoCode")
}

pub fn get_sohocode_config_path() -> PathBuf {
    get_sohocode_dir().join("config.json")
}

pub fn read_sohocode_config() -> Result<Value, AppError> {
    let path = get_sohocode_config_path();
    if !path.exists() {
        return Ok(json!({ "providers": {} }));
    }

    let content = std::fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
    if content.trim().is_empty() {
        return Ok(json!({ "providers": {} }));
    }

    json5::from_str(&content)
        .map_err(|e| AppError::Config(format!("Failed to parse SohoCode config: {}: {e}", path.display())))
}

pub fn write_sohocode_config(config: &Value) -> Result<(), AppError> {
    let path = get_sohocode_config_path();
    write_json_file(&path, config)
}

fn normalize_models_for_write(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items.into_iter()
                .filter_map(|item| match item {
                    Value::String(model) => {
                        let trimmed = model.trim();
                        (!trimmed.is_empty()).then(|| Value::String(trimmed.to_string()))
                    }
                    Value::Object(obj) => obj
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .map(|id| Value::String(id.to_string())),
                    _ => None,
                })
                .collect(),
        ),
        other => other,
    }
}

fn normalize_models_for_read(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items.into_iter()
                .filter_map(|item| match item {
                    Value::String(model) => {
                        let trimmed = model.trim();
                        (!trimmed.is_empty()).then(|| json!({ "id": trimmed, "name": trimmed }))
                    }
                    Value::Object(obj) => Some(Value::Object(obj)),
                    _ => None,
                })
                .collect(),
        ),
        other => other,
    }
}

fn normalize_provider_for_write(config: Value) -> Value {
    let mut obj = match config {
        Value::Object(obj) => obj,
        _ => Map::new(),
    };

    obj.entry("type".to_string())
        .or_insert_with(|| Value::String("openai-compatible".to_string()));

    if let Some(models) = obj.remove("models") {
        obj.insert("models".to_string(), normalize_models_for_write(models));
    }

    Value::Object(obj)
}

fn normalize_provider_for_read(config: Value) -> Value {
    let mut obj = match config {
        Value::Object(obj) => obj,
        _ => Map::new(),
    };

    if let Some(models) = obj.remove("models") {
        obj.insert("models".to_string(), normalize_models_for_read(models));
    }

    Value::Object(obj)
}

pub fn get_providers() -> Result<Map<String, Value>, AppError> {
    let config = read_sohocode_config()?;
    let mut result = Map::new();

    if let Some(providers) = config.get("providers").and_then(Value::as_object) {
        for (id, provider) in providers {
            result.insert(id.clone(), normalize_provider_for_read(provider.clone()));
        }
    }

    Ok(result)
}

pub fn get_provider(id: &str) -> Result<Option<Value>, AppError> {
    Ok(get_providers()?.remove(id))
}

pub fn set_provider(id: &str, config: Value) -> Result<(), AppError> {
    let mut full_config = read_sohocode_config()?;

    if full_config.get("providers").is_none() {
        full_config["providers"] = json!({});
    }

    if let Some(providers) = full_config.get_mut("providers").and_then(Value::as_object_mut) {
        providers.insert(id.to_string(), normalize_provider_for_write(config));
    }

    write_sohocode_config(&full_config)
}

pub fn remove_provider(id: &str) -> Result<(), AppError> {
    let mut config = read_sohocode_config()?;

    if let Some(providers) = config.get_mut("providers").and_then(Value::as_object_mut) {
        providers.remove(id);
    }

    write_sohocode_config(&config)
}

pub fn apply_switch_defaults(provider_id: &str, provider_settings: &Value) -> Result<(), AppError> {
    let mut config = read_sohocode_config()?;
    let provider = normalize_provider_for_write(provider_settings.clone());

    if config.get("providers").is_none() {
        config["providers"] = json!({});
    }

    if let Some(providers) = config.get_mut("providers").and_then(Value::as_object_mut) {
        providers.insert(provider_id.to_string(), provider.clone());
    }

    config["default_provider"] = Value::String(provider_id.to_string());

    if let Some(base_url) = provider.get("base_url").and_then(Value::as_str) {
        config["api_base"] = Value::String(base_url.to_string());
    }
    if let Some(api_key) = provider.get("api_key").and_then(Value::as_str) {
        config["api_key"] = Value::String(api_key.to_string());
    }
    if let Some(models) = provider.get("models").cloned() {
        config["models"] = models.clone();
        if let Some(first_model) = models
            .as_array()
            .and_then(|items| items.first())
            .and_then(Value::as_str)
        {
            config["model"] = Value::String(first_model.to_string());
        }
    }

    write_sohocode_config(&config)
}
