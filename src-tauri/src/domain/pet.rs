//! Pure wardrobe rules. Rendering metadata and unlock rules share one catalog.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
pub struct Item {
    pub id: String,
    pub slot: String,
    pub source: String,
    pub weight: u32,
    pub cost: u32,
    pub metric: Option<String>,
    pub target: Option<u64>,
}
pub fn catalog() -> &'static [Item] {
    static ITEMS: OnceLock<Vec<Item>> = OnceLock::new();
    ITEMS.get_or_init(|| {
        serde_json::from_str(include_str!("../../../src/features/pet/catalog.json"))
            .expect("embedded pet catalog must be valid")
    })
}
pub type Outfit = BTreeMap<String, String>;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wardrobe {
    pub owned: Vec<String>,
    pub equipped: Outfit,
    pub presets: Vec<Outfit>,
    pub seconds: u64,
    pub days: u32,
    pub last_day: String,
    pub daily_rolls: u32,
    pub roll_seconds: u64,
    pub misses: u32,
    pub fragments: u32,
    pub tone: String,
}
impl Wardrobe {
    pub fn from_legacy(skin: &str, unlocked: &[String]) -> Self {
        let mut owned: Vec<String> = catalog()
            .iter()
            .filter(|i| i.source == "starter")
            .map(|i| i.id.clone())
            .collect();
        let mut equipped = Outfit::new();
        if skin == "mage" || unlocked.iter().any(|s| s == "mage") {
            owned.push("circlet".into());
            if skin == "mage" {
                equipped.insert("head".into(), "circlet".into());
            }
        }
        Self {
            owned,
            equipped,
            presets: vec![Outfit::new(); 3],
            seconds: 0,
            days: 0,
            last_day: String::new(),
            daily_rolls: 0,
            roll_seconds: 0,
            misses: 0,
            fragments: 0,
            tone: "mixed".into(),
        }
    }
    fn grant(&mut self, id: &str, achievement: bool) -> Reward {
        let duplicate = self.owned.iter().any(|owned| owned == id);
        if duplicate {
            self.fragments = self.fragments.saturating_add(4);
        } else {
            self.owned.push(id.into());
        }
        Reward {
            id: id.into(),
            duplicate,
            achievement,
        }
    }
    /// Randomness and civil time are provided by the runtime, never the webview.
    pub fn advance(
        &mut self,
        seconds: u64,
        day: &str,
        mut random: impl FnMut(u32) -> u32,
    ) -> Vec<Reward> {
        let mut rewards = Vec::new();
        if seconds == 0 {
            return rewards;
        }
        if day > self.last_day.as_str() {
            self.last_day = day.into();
            self.days = self.days.saturating_add(1);
            self.daily_rolls = 0;
        }
        self.seconds = self.seconds.saturating_add(seconds);
        // Only credit time that can be used before today's last opportunity.
        // Any time after the twelfth roll belongs solely to achievements.
        let remaining = u64::from(12_u32.saturating_sub(self.daily_rolls)) * 600;
        if remaining > 0 {
            self.roll_seconds = self.roll_seconds.saturating_add(seconds).min(remaining);
        }
        while self.roll_seconds >= 600 && self.daily_rolls < 12 {
            self.roll_seconds -= 600;
            self.daily_rolls += 1;
            if self.misses >= 15 || random(100) < 10 {
                self.misses = 0;
                let total = catalog().iter().map(|i| i.weight).sum();
                let mut choice = random(total);
                for item in catalog().iter().filter(|i| i.weight > 0) {
                    if choice < item.weight {
                        rewards.push(self.grant(&item.id, false));
                        break;
                    }
                    choice -= item.weight;
                }
            } else {
                self.misses += 1;
                // Even a miss advances targeted redemption; no endless duplicate grind.
                self.fragments = self.fragments.saturating_add(1);
            }
        }
        if self.daily_rolls >= 12 {
            self.roll_seconds = 0;
        }
        for item in catalog().iter().filter(|i| i.source == "achievement") {
            let progress = if item.metric.as_deref() == Some("days") {
                self.days as u64
            } else {
                self.seconds
            };
            if progress >= item.target.unwrap_or(u64::MAX) && !self.owned.contains(&item.id) {
                rewards.push(self.grant(&item.id, true));
            }
        }
        rewards
    }
    pub fn apply(&mut self, action: Action) -> Result<(), String> {
        match action {
            Action::Equip { id } => {
                let item = catalog().iter().find(|i| i.id == id).ok_or("装扮不存在")?;
                if !self.owned.contains(&id) {
                    return Err("尚未拥有该装扮".into());
                }
                self.equipped.insert(item.slot.clone(), id);
            }
            Action::Unequip { slot } => {
                self.equipped.remove(&slot);
            }
            Action::Clear => self.equipped.clear(),
            Action::Redeem { id } => {
                let item = catalog().iter().find(|i| i.id == id).ok_or("装扮不存在")?;
                if item.source != "random" || self.owned.contains(&id) {
                    return Err("该装扮不可兑换".into());
                }
                if self.fragments < item.cost {
                    return Err("装扮碎片不足".into());
                }
                self.fragments -= item.cost;
                self.owned.push(id);
            }
            Action::Tone { tone } => {
                if !["mixed", "gentle", "snarky"].contains(&tone.as_str()) {
                    return Err("未知语气".into());
                }
                self.tone = tone;
            }
            Action::SavePreset { index } => {
                if index >= 3 {
                    return Err("搭配栏不存在".into());
                }
                self.presets.resize_with(3, Outfit::new);
                self.presets[index] = self.equipped.clone();
            }
            Action::LoadPreset { index } => {
                self.equipped = self.presets.get(index).ok_or("搭配栏不存在")?.clone();
            }
        }
        Ok(())
    }
}
#[derive(Clone, Serialize)]
pub struct Reward {
    pub id: String,
    pub duplicate: bool,
    pub achievement: bool,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Equip { id: String },
    Unequip { slot: String },
    Clear,
    Redeem { id: String },
    Tone { tone: String },
    SavePreset { index: usize },
    LoadPreset { index: usize },
}
