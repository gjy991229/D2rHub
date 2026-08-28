use crate::error::AppError;
use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

const API_URL: &str = "https://api.d2-trade.com.cn/api/query/tz_online/zh-cn";

static CURRENT_TERROR_ZONE: OnceLock<RwLock<Option<TerrorZoneForecast>>> = OnceLock::new();

fn current_terror_zone_cache() -> &'static RwLock<Option<TerrorZoneForecast>> {
    CURRENT_TERROR_ZONE.get_or_init(|| RwLock::new(None))
}

#[derive(Debug, Deserialize)]
struct TerrorZoneApiResponse {
    status: String,
    data: Option<Vec<TerrorZoneApiItem>>,
}

#[derive(Debug, Clone, Deserialize)]
struct TerrorZoneApiItem {
    time: i64,
    end_time: i64,
    name: ZoneName,
    immunities: Option<Vec<String>>,
    #[serde(rename = "tier-exp")]
    tier_exp: Option<String>,
    #[serde(rename = "tier-loot")]
    tier_loot: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ZoneName {
    Single(String),
    Multiple(Vec<String>),
}

impl ZoneName {
    fn names(&self) -> Vec<String> {
        match self {
            ZoneName::Single(name) => vec![name.clone()],
            ZoneName::Multiple(names) => names.clone(),
        }
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TerrorZoneForecast {
    pub start_time: i64,
    pub end_time: i64,
    pub display_time: String,
    pub location_name: String,
    pub location_detail: String,
    pub tier_exp: String,
    pub tier_loot: String,
    pub immunities: Vec<TerrorZoneImmunity>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerrorZoneSnapshot {
    pub current: Option<TerrorZoneForecast>,
    pub next: Option<TerrorZoneForecast>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerrorZoneImmunity {
    pub code: String,
    pub label: String,
    pub color: String,
}

/// Returns the last successfully fetched current TZ while it is still valid.
/// The audio capture thread uses this synchronous snapshot so network latency
/// can never interrupt process-loopback capture.
pub(crate) fn cached_current_terror_zone() -> Option<TerrorZoneForecast> {
    let now = Utc::now().timestamp();
    current_terror_zone_cache()
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()
        .filter(|zone| zone.start_time <= now && zone.end_time > now)
        .cloned()
}

fn cache_current_terror_zone(current: Option<TerrorZoneForecast>) {
    *current_terror_zone_cache()
        .write()
        .unwrap_or_else(|error| error.into_inner()) = current;
}

fn immunity_meta(code: &str) -> TerrorZoneImmunity {
    let (label, color) = match code {
        "f" => ("火", "#FF4D4D"),
        "c" => ("冰", "#6969FF"),
        "l" => ("电", "#FFFF00"),
        "p" => ("毒", "#00FF00"),
        "m" => ("魔", "#FFA500"),
        "ph" => ("物", "#FFFFFF"),
        _ => (code, "#CCCCCC"),
    };

    TerrorZoneImmunity {
        code: code.to_string(),
        label: label.to_string(),
        color: color.to_string(),
    }
}

fn format_shanghai_time(timestamp: i64) -> String {
    let Some(offset) = FixedOffset::east_opt(8 * 3600) else {
        return "--:--".to_string();
    };

    DateTime::<Utc>::from_timestamp(timestamp, 0)
        .map(|time| time.with_timezone(&offset).format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".to_string())
}

fn build_forecast(item: TerrorZoneApiItem) -> Option<TerrorZoneForecast> {
    let names = item.name.names();
    let location_name = names.first()?.clone();
    let location_detail = names.join(" / ");

    Some(TerrorZoneForecast {
        start_time: item.time,
        end_time: item.end_time,
        display_time: format_shanghai_time(item.time),
        location_name,
        location_detail,
        tier_exp: item.tier_exp.unwrap_or_else(|| "-".to_string()),
        tier_loot: item.tier_loot.unwrap_or_else(|| "-".to_string()),
        immunities: item
            .immunities
            .unwrap_or_default()
            .iter()
            .map(|code| immunity_meta(code))
            .collect(),
    })
}

fn select_current_zone(items: &[TerrorZoneApiItem], now: i64) -> Option<TerrorZoneApiItem> {
    items
        .iter()
        .filter(|item| item.time <= now && item.end_time > now)
        .max_by_key(|item| item.time)
        .cloned()
}

fn select_next_zone(items: &[TerrorZoneApiItem], now: i64) -> Option<TerrorZoneApiItem> {
    items
        .iter()
        .filter(|item| item.time > now)
        .min_by_key(|item| item.time)
        .cloned()
}

async fn fetch_terror_zone_items() -> Result<Vec<TerrorZoneApiItem>, AppError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|err| AppError::FileError(format!("创建 TZ 请求客户端失败: {}", err)))?;

    let response = client
        .get(API_URL)
        .send()
        .await
        .map_err(|err| AppError::FileError(format!("获取 TZ 预报失败: {}", err)))?;

    if !response.status().is_success() {
        return Err(AppError::FileError(format!(
            "获取 TZ 预报失败: HTTP {}",
            response.status()
        )));
    }

    let body = response
        .text()
        .await
        .map_err(|err| AppError::FileError(format!("读取 TZ 预报失败: {}", err)))?;

    let api_data: TerrorZoneApiResponse = serde_json::from_str(&body)?;
    if api_data.status != "ok" {
        return Err(AppError::FileError("TZ 预报接口返回异常".to_string()));
    }

    Ok(api_data.data.unwrap_or_default())
}

#[tauri::command]
pub async fn get_terror_zone_snapshot() -> Result<TerrorZoneSnapshot, AppError> {
    let items = fetch_terror_zone_items().await?;
    let now = Utc::now().timestamp();
    let current = select_current_zone(&items, now).and_then(build_forecast);
    cache_current_terror_zone(current.clone());

    Ok(TerrorZoneSnapshot {
        current,
        next: select_next_zone(&items, now).and_then(build_forecast),
    })
}

#[tauri::command]
pub async fn get_next_terror_zone() -> Result<Option<TerrorZoneForecast>, AppError> {
    let items = fetch_terror_zone_items().await?;

    Ok(select_next_zone(&items, Utc::now().timestamp()).and_then(build_forecast))
}
