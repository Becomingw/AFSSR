//! Deterministic models.dev matching and metadata conversion shared by import and refresh.
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub struct CatalogModel<'a> {
    pub provider_id: &'a str,
    pub model_id: &'a str,
    pub provider: &'a Value,
    pub model: &'a Value,
}

pub struct CatalogIndex<'a> {
    by_id: BTreeMap<String, Vec<CatalogModel<'a>>>,
}

impl<'a> CatalogIndex<'a> {
    pub fn new(payload: &'a Value) -> Self {
        let mut by_id: BTreeMap<String, Vec<CatalogModel<'a>>> = BTreeMap::new();
        if let Some(providers) = payload.as_object() {
            for (provider_id, provider) in providers {
                if let Some(models) = provider.get("models").and_then(Value::as_object) {
                    for (model_id, model) in models {
                        let item = CatalogModel {
                            provider_id,
                            model_id,
                            provider,
                            model,
                        };
                        by_id
                            .entry(model_id.to_ascii_lowercase())
                            .or_default()
                            .push(item);
                    }
                }
            }
        }
        Self { by_id }
    }

    pub fn find(&self, name: &str, config: Option<&Value>) -> Option<CatalogModel<'a>> {
        let source = config.and_then(|v| v.get("models_dev_pricing_source"));
        if let Some(source) = source {
            // A pinned source must not silently switch vendor or pricing if it disappears.
            let provider = source.get("provider_id")?.as_str()?;
            let model = source
                .get("model_id")
                .and_then(Value::as_str)
                .unwrap_or(name);
            return self
                .by_id
                .get(&model.to_ascii_lowercase())?
                .iter()
                .find(|item| item.provider_id == provider)
                .copied();
        }
        let normalized = name.trim().to_ascii_lowercase();
        let candidates = self.by_id.get(&normalized).or_else(|| {
            normalized
                .split_once('/')
                .and_then(|(_, suffix)| self.by_id.get(suffix))
        })?;
        if let Some((prefix, _)) = normalized.split_once('/') {
            if let Some(item) = candidates
                .iter()
                .find(|item| item.provider_id.eq_ignore_ascii_case(prefix))
            {
                return Some(*item);
            }
        }
        if candidates.len() == 1 {
            return candidates.first().copied();
        }
        let mut official = candidates
            .iter()
            .filter(|item| item.provider["official"] == true);
        let selected = official.next()?;
        if official.next().is_none() {
            Some(*selected)
        } else {
            None
        }
    }
}

fn price(v: &Value) -> Option<f64> {
    v.as_f64().filter(|v| v.is_finite() && *v >= 0.0)
}

fn token_prices(cost: &Value) -> Option<Map<String, Value>> {
    let mut prices = Map::new();
    for (source, target) in [
        ("input", "input_price_per_1m"),
        ("output", "output_price_per_1m"),
        ("cache_read", "cache_read_price_per_1m"),
        ("cache_write", "cache_creation_price_per_1m"),
    ] {
        match cost.get(source) {
            Some(value) => {
                prices.insert(target.to_string(), json!(price(value)?));
            }
            None if source == "input" || source == "output" => return None,
            None => {}
        }
    }
    for (special, base) in [
        ("reasoning", "output"),
        ("input_audio", "input"),
        ("output_audio", "output"),
    ] {
        if let Some(value) = cost.get(special) {
            if price(value)? != price(cost.get(base)?)? {
                return None;
            }
        }
    }
    Some(prices)
}

pub fn catalog_pricing(cost: &Value) -> Option<Value> {
    let base = token_prices(cost)?;
    let mut bands = BTreeMap::new();
    if let Some(raw) = cost.get("tiers") {
        for tier in raw.as_array()? {
            if tier["tier"]["type"] != "context" {
                return None;
            }
            let size = tier["tier"]["size"]
                .as_u64()
                .filter(|n| *n <= 9_007_199_254_740_991)?;
            if bands.insert(size, token_prices(tier)?).is_some() {
                return None;
            }
        }
    } else if cost.get("context_over_200k").is_some() {
        return None;
    }
    let mut tiers = Vec::new();
    let mut current = base;
    for (size, next) in bands {
        if size > 0 {
            current.insert("up_to".to_string(), json!(size - 1));
            tiers.push(Value::Object(current));
        }
        current = next;
    }
    current.insert("up_to".to_string(), Value::Null);
    tiers.push(Value::Object(current));
    Some(json!({"tiers": tiers}))
}

impl CatalogModel<'_> {
    pub fn capabilities(&self, existing: Option<&Value>) -> Option<Value> {
        let mut values = existing
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if self.model_id.to_ascii_lowercase().contains("embedding")
            && !values.iter().any(|v| v == "embedding")
        {
            values.push(json!("embedding"));
        }
        if self
            .model
            .pointer("/modalities/output")
            .or_else(|| self.model.get("output"))
            .and_then(Value::as_array)
            .is_some_and(|v| v.iter().any(|v| v == "image"))
            && !values.iter().any(|v| v == "image_generation")
        {
            values.push(json!("image_generation"));
        }
        if values.is_empty() {
            existing.cloned()
        } else {
            Some(Value::Array(values))
        }
    }

    pub fn pricing(&self) -> Option<Value> {
        let mut pricing = catalog_pricing(self.model.get("cost")?)?;
        let mut processing = Map::new();
        if let Some(modes) = self
            .model
            .pointer("/experimental/modes")
            .and_then(Value::as_object)
        {
            for (mode, data) in modes {
                let body = data.pointer("/provider/body");
                let raw_key =
                    if body.and_then(|b| b.get("speed")).and_then(Value::as_str) == Some("fast") {
                        "fast"
                    } else {
                        body.and_then(|b| b.get("service_tier"))
                            .and_then(Value::as_str)
                            .unwrap_or(mode)
                    };
                let key = raw_key.trim().to_ascii_lowercase();
                let explicit_tier = body.and_then(|b| b.get("service_tier")).is_some();
                if key.is_empty()
                    || key.len() > 64
                    || ["auto", "default", "standard"].contains(&key.as_str())
                    || (!explicit_tier
                        && !["fast", "priority", "flex", "batch"].contains(&key.as_str()))
                {
                    continue;
                }
                let Some(cost) = data.get("cost") else {
                    continue;
                };
                if cost.get("tiers").is_some() || cost.get("context_over_200k").is_some() {
                    continue;
                }
                {
                    let mode_pricing = catalog_pricing(cost)?;
                    // Zero experimental costs are commonly a missing-price marker, not free service.
                    if cost["input"].as_f64() == Some(0.0) && cost["output"].as_f64() == Some(0.0) {
                        continue;
                    }
                    processing.entry(key).or_insert(mode_pricing);
                }
            }
        }
        if !processing.is_empty() {
            pricing["processing_tiers"] = Value::Object(processing);
        }
        Some(pricing)
    }

    pub fn config(&self, existing: Option<&Value>) -> Value {
        let mut config = existing
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (source, target) in [
            ("reasoning", "extended_thinking"),
            ("tool_call", "function_calling"),
            ("structured_output", "structured_output"),
            ("temperature", "temperature"),
            ("attachment", "attachment"),
            ("open_weights", "open_weights"),
            ("knowledge", "knowledge_cutoff"),
            ("release_date", "release_date"),
            ("family", "family"),
        ] {
            if let Some(value) = self.model.get(source).filter(|v| !v.is_null()) {
                config.insert(target.to_string(), value.clone());
            }
        }
        for (source, target) in [("context", "context_limit"), ("output", "output_limit")] {
            if let Some(value) = self
                .model
                .get("limit")
                .and_then(|v| v.get(source))
                .and_then(Value::as_u64)
                .filter(|n| *n > 0)
            {
                config.insert(target.to_string(), json!(value));
            }
        }
        for (source, target) in [
            ("input", "input_modalities"),
            ("output", "output_modalities"),
        ] {
            if let Some(value) = self
                .model
                .get("modalities")
                .and_then(|v| v.get(source))
                .or_else(|| self.model.get(source))
                .filter(|v| v.is_array())
            {
                config.insert(target.to_string(), value.clone());
                if source == "input" {
                    config.insert(
                        "vision".to_string(),
                        json!(value
                            .as_array()
                            .is_some_and(|v| v.iter().any(|v| v == "image"))),
                    );
                }
            }
        }
        // Catalog API hints describe the native upstream; they do not override channel routing.
        let npm = self
            .model
            .pointer("/provider/npm")
            .or_else(|| self.provider.get("npm"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let format = match npm {
            "@ai-sdk/anthropic" => "claude:messages",
            "@ai-sdk/google" => "gemini:generate_content",
            "@ai-sdk/openai" => "openai:responses",
            "@ai-sdk/openai-compatible" => "openai:chat",
            _ => "",
        };
        if !format.is_empty() {
            config.insert("catalog_api_formats".to_string(), json!([format]));
        }
        config.insert(
            "models_dev_pricing_source".to_string(),
            json!({"provider_id":self.provider_id,"provider_name":self.provider.get("name").and_then(Value::as_str).unwrap_or(self.provider_id),"model_id":self.model_id}),
        );
        config.insert(
            "catalog_metadata_status".to_string(),
            json!(if self.pricing().is_some() {
                "matched"
            } else {
                "pricing_unavailable"
            }),
        );
        config.entry("streaming").or_insert(json!(!self
            .model_id
            .to_ascii_lowercase()
            .contains("embedding")));
        Value::Object(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matching_uses_pinned_or_unique_official_sources_without_fuzzy_aliases() {
        let data = json!({"vendor":{"official":true,"models":{"model":{"name":"Model"}}}, "relay":{"models":{"model":{},"other":{}}}});
        let index = CatalogIndex::new(&data);
        assert_eq!(index.find("model", None).unwrap().provider_id, "vendor");
        assert_eq!(
            index.find("relay/model", None).unwrap().provider_id,
            "relay"
        );
        assert!(index.find("model-fast", None).is_none());
        assert!(index.find("model", Some(&json!({"models_dev_pricing_source":{"provider_id":"deleted","model_id":"model"}}))).is_none());
        assert_eq!(index.find("alias", Some(&json!({"models_dev_pricing_source":{"provider_id":"vendor","model_id":"model"}}))).unwrap().model_id, "model");
    }
    #[test]
    fn preserves_exact_context_boundaries_and_cache_costs_and_explicit_free() {
        let pricing = catalog_pricing(&json!({"input":1,"output":2,"cache_read":0.1,"tiers":[{"input":3,"output":4,"tier":{"type":"context","size":200000}}]})).unwrap();
        assert_eq!(pricing["tiers"][0]["up_to"], 199999);
        assert_eq!(pricing["tiers"][0]["cache_read_price_per_1m"], 0.1);
        assert_eq!(pricing["tiers"][1]["input_price_per_1m"], 3.0);
        assert!(pricing["tiers"][1]["up_to"].is_null());
        assert!(catalog_pricing(&json!({"input":0,"output":0})).is_some());
        assert!(catalog_pricing(&json!({"output":0})).is_none());
        assert!(catalog_pricing(&json!({"input":1,"output":2,"reasoning":3})).is_none());
        assert!(catalog_pricing(&json!({"input":1,"output":2,"context_over_200k":{}})).is_none());
    }
    #[test]
    fn metadata_updates_preserve_custom_settings_and_use_real_protocol_hints() {
        let data = json!({"vendor":{"npm":"@ai-sdk/anthropic","models":{"model":{"tool_call":true,"limit":{"context":200000},"modalities":{"input":["text","image"]},"cost":{"input":1,"output":2}}}}});
        let item = CatalogIndex::new(&data).find("model", None).unwrap();
        let config = item.config(Some(&json!({"model_mappings":["alias"],"custom":42})));
        assert_eq!(config["custom"], 42);
        assert_eq!(config["context_limit"], 200000);
        assert_eq!(config["vision"], true);
        assert_eq!(config["catalog_api_formats"], json!(["claude:messages"]));
        assert!(config.get("api_formats").is_none());
    }

    #[test]
    fn processing_prices_preserve_flat_overrides_and_reject_unrepresentable_costs() {
        let mut payload = json!({"vendor":{"models":{"model":{"cost":{"input":1,"output":2},
            "experimental":{"modes":{"priority":{"cost":{"input":3,"output":6},"provider":{"body":{"service_tier":"priority"}}}}}
        }}}});
        let index = CatalogIndex::new(&payload);
        let pricing = index.find("model", None).unwrap().pricing().unwrap();
        assert_eq!(
            pricing["processing_tiers"]["priority"]["tiers"][0]["input_price_per_1m"],
            3.0
        );
        payload["vendor"]["models"]["model"]["experimental"]["modes"]["priority"]["cost"]
            ["reasoning"] = json!(99);
        assert!(CatalogIndex::new(&payload)
            .find("model", None)
            .unwrap()
            .pricing()
            .is_none());
    }
}
