//! Progress of Link: the number of completed chat turns, and the gear tier
//! that it unlocks.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A gear tier: the completed turns that unlock it, and its gear. The gear of
/// each tier comes from `tools/link-extract/extract_all.sh`.
pub struct TierInfo {
    pub needs: u32,
    pub name: &'static str,
}

pub const TIERS: [TierInfo; 5] = [
    TierInfo {
        needs: 0,
        name: "Fighter's Sword",
    },
    TierInfo {
        needs: 10,
        name: "Master Sword",
    },
    TierInfo {
        needs: 30,
        name: "Blue Mail",
    },
    TierInfo {
        needs: 75,
        name: "Tempered Sword",
    },
    TierInfo {
        needs: 150,
        name: "Golden Sword",
    },
];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    /// Number of chat turns that ended with success.
    pub completed: u32,
}

impl Progress {
    fn path() -> Option<PathBuf> {
        Some(dirs::data_dir()?.join("pixelpal").join("progress.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = crate::files::write_atomic(&path, s.as_bytes());
        }
    }

    /// The unlocked gear tier.
    pub fn tier(&self) -> usize {
        TIERS
            .iter()
            .rposition(|t| self.completed >= t.needs)
            .unwrap_or(0)
    }

    /// A line for the settings window: the turns, the tier, and the next tier.
    pub fn summary(&self) -> String {
        let tier = self.tier();
        let mut s = format!("{} completed chats: {}", self.completed, TIERS[tier].name);
        if let Some(next) = TIERS.get(tier + 1) {
            s += &format!(". {} at {} chats.", next.name, next.needs);
        } else {
            s += ". All gear unlocked.";
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_follow_the_completed_turns() {
        let tier = |completed| Progress { completed }.tier();
        assert_eq!(tier(0), 0);
        assert_eq!(tier(9), 0);
        assert_eq!(tier(10), 1);
        assert_eq!(tier(149), 3);
        assert_eq!(tier(1000), 4);
    }
}
