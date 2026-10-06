use crate::protocol::ClientFrame;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AutomationKind {
    AutoPotion,
    AutoRevive,
    AutoBall,
    AutoVendaLoot,
    AutoHunt,
    AutoBuyPotion,
    AutoBuyBall,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationConfig {
    pub kind: AutomationKind,
    pub enabled: bool,
    pub hp_threshold: Option<f64>,
}
#[derive(Default)]
pub struct AutomationManager {
    configs: std::collections::HashMap<AutomationKind, AutomationConfig>,
}
impl AutomationManager {
    pub fn set(&mut self, config: AutomationConfig) {
        self.configs.insert(config.kind, config);
    }
    pub fn enabled(&self, kind: AutomationKind) -> bool {
        self.configs.get(&kind).is_some_and(|config| config.enabled)
    }
    pub fn confirmed_command(&self, kind: AutomationKind, enabled: bool) -> Option<ClientFrame> {
        match kind {
            AutomationKind::AutoVendaLoot => Some(ClientFrame::AutoSaleLoot { ativo: enabled }),
            _ => None,
        }
    }
}
