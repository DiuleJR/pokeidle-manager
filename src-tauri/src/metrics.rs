use crate::protocol::{BattleEvent, DeathEvent};
use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};

const WINDOW: Duration = Duration::from_secs(15 * 60);
#[derive(Debug, Clone)]
struct Sample {
    at: Instant,
    cumulative_xp: u64,
    cumulative_gold: u64,
}
#[derive(Debug, Clone)]
struct ConsumptionSample {
    at: Instant,
    item_id: String,
    quantity: u64,
}
#[derive(Debug, Clone, Default)]
pub struct MetricsEngine {
    samples: VecDeque<Sample>,
    recorded_xp: u64,
    recorded_gold: u64,
    trimmed_xp: u64,
    trimmed_gold: u64,
    kills: u64,
    captures: u64,
    potions: BTreeMap<String, u64>,
    potion_samples: VecDeque<ConsumptionSample>,
    balls: BTreeMap<String, u64>,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetricView {
    pub xp_per_hour: u64,
    pub gold_per_hour: u64,
    pub kills: u64,
    pub captures: u64,
}
impl MetricsEngine {
    pub fn potions(&self) -> &BTreeMap<String, u64> {
        &self.potions
    }
    pub fn balls(&self) -> &BTreeMap<String, u64> {
        &self.balls
    }
    pub fn on_battle_event(&mut self, event: &BattleEvent, farm_active: bool) {
        match event {
            BattleEvent::Death(DeathEvent {
                quem,
                trainer_xp,
                ouro,
                auto_sale_gold,
                ..
            }) if quem == "selvagem" => {
                self.kills += 1;
                self.record(*trainer_xp, ouro.saturating_add(*auto_sale_gold));
            }
            BattleEvent::Ball {
                sucesso, ball_id, ..
            } => {
                *self.balls.entry(ball_id.to_string()).or_default() += 1;
                if *sucesso {
                    self.captures += 1;
                }
            }
            _ => {}
        }
        if !farm_active {
            if let Some(last) = self.samples.back() {
                self.trimmed_xp = last.cumulative_xp;
                self.trimmed_gold = last.cumulative_gold;
            } else {
                self.trimmed_xp = self.recorded_xp;
                self.trimmed_gold = self.recorded_gold;
            }
            self.samples.clear();
        }
    }
    pub fn record_potion(&mut self, item_id: u64) {
        *self.potions.entry(item_id.to_string()).or_default() += 1;
        self.potion_samples.push_back(ConsumptionSample {
            at: Instant::now(),
            item_id: item_id.to_string(),
            quantity: 1,
        });
        self.trim();
    }
    /// Rolling rate only becomes present after enough observation time to avoid
    /// presenting a single newly-observed use as an inflated hourly average.
    pub fn potions_per_hour(&mut self) -> Option<u64> {
        let rates = self.potion_usage_per_hour();
        (!rates.is_empty()).then(|| rates.values().sum())
    }
    /// Per-item rolling rates retain the observed Potion identity for future
    /// stock planning without promoting an inferred delta to a protocol frame.
    pub fn potion_usage_per_hour(&mut self) -> BTreeMap<String, u64> {
        self.trim();
        let Some(first) = self.potion_samples.front() else {
            return BTreeMap::new();
        };
        let elapsed = first.at.elapsed();
        if elapsed < Duration::from_secs(60) {
            return BTreeMap::new();
        }
        let mut rates = BTreeMap::new();
        for sample in &self.potion_samples {
            *rates.entry(sample.item_id.clone()).or_insert(0_u64) += sample.quantity;
        }
        rates
            .into_iter()
            .map(|(item_id, used)| {
                (
                    item_id,
                    (used as f64 * 3600.0 / elapsed.as_secs_f64()) as u64,
                )
            })
            .collect()
    }
    pub fn view(&mut self, farm_active: bool) -> MetricView {
        if !farm_active {
            return MetricView {
                kills: self.kills,
                captures: self.captures,
                ..Default::default()
            };
        }
        self.trim();
        self.view_read_only(true)
    }
    /// Computes the current rate view without trimming or otherwise changing
    /// the rolling samples. Mobile projections use this read-only path so that
    /// observing metrics cannot mutate the account runtime.
    pub fn view_read_only(&self, farm_active: bool) -> MetricView {
        if !farm_active {
            return MetricView {
                kills: self.kills,
                captures: self.captures,
                ..Default::default()
            };
        }

        let Some((first_at, xp, gold)) = self.read_only_window() else {
            return MetricView {
                kills: self.kills,
                captures: self.captures,
                ..Default::default()
            };
        };
        let elapsed = Instant::now()
            .duration_since(first_at)
            .as_secs_f64()
            .max(1.0);
        MetricView {
            xp_per_hour: (xp as f64 * 3600.0 / elapsed) as u64,
            gold_per_hour: (gold as f64 * 3600.0 / elapsed) as u64,
            kills: self.kills,
            captures: self.captures,
        }
    }
    fn record(&mut self, xp: u64, gold: u64) {
        self.recorded_xp = self.recorded_xp.saturating_add(xp);
        self.recorded_gold = self.recorded_gold.saturating_add(gold);
        self.samples.push_back(Sample {
            at: Instant::now(),
            cumulative_xp: self.recorded_xp,
            cumulative_gold: self.recorded_gold,
        });
        self.trim();
    }

    /// Returns the current rolling totals without mutation in O(log n). This
    /// keeps read-only mobile projections from walking every battle sample
    /// while holding the account map lock.
    fn read_only_window(&self) -> Option<(Instant, u64, u64)> {
        let now = Instant::now();
        let cutoff = now.checked_sub(WINDOW).unwrap_or(now);
        let mut low = 0;
        let mut high = self.samples.len();
        while low < high {
            let middle = low + (high - low) / 2;
            if self.samples[middle].at < cutoff {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let first = self.samples.get(low)?;
        let last = self.samples.back()?;
        let previous_xp = if low == 0 {
            self.trimmed_xp
        } else {
            self.samples[low - 1].cumulative_xp
        };
        let previous_gold = if low == 0 {
            self.trimmed_gold
        } else {
            self.samples[low - 1].cumulative_gold
        };
        Some((
            first.at,
            last.cumulative_xp.saturating_sub(previous_xp),
            last.cumulative_gold.saturating_sub(previous_gold),
        ))
    }

    fn trim(&mut self) {
        while self
            .samples
            .front()
            .is_some_and(|sample| sample.at.elapsed() > WINDOW)
        {
            if let Some(sample) = self.samples.pop_front() {
                self.trimmed_xp = sample.cumulative_xp;
                self.trimmed_gold = sample.cumulative_gold;
            }
        }
        while self
            .potion_samples
            .front()
            .is_some_and(|sample| sample.at.elapsed() > WINDOW)
        {
            self.potion_samples.pop_front();
        }
    }
    pub fn consumed_per_hour(
        &self,
        item: &str,
        current_quantity: u64,
        elapsed: Duration,
    ) -> Option<(f64, f64)> {
        let used = *self.potions.get(item).or_else(|| self.balls.get(item))?;
        if elapsed < Duration::from_secs(60) {
            return None;
        }
        let rate = used as f64 * 3600.0 / elapsed.as_secs_f64();
        Some((rate, current_quantity as f64 / rate.max(f64::MIN_POSITIVE)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::BattleEvent;
    #[test]
    fn inactive_farm_reports_zero_rates() {
        let mut metrics = MetricsEngine::default();
        metrics.on_battle_event(&BattleEvent::Death(DeathEvent::wild(100, 20)), true);
        assert_eq!(metrics.view(false).xp_per_hour, 0);
    }
    #[test]
    fn consumption_requires_observation_window() {
        let mut metrics = MetricsEngine::default();
        metrics.record_potion(204);
        assert!(
            metrics
                .consumed_per_hour("204", 20, Duration::from_secs(30))
                .is_none()
        );
        assert!(
            metrics
                .consumed_per_hour("204", 20, Duration::from_secs(60))
                .is_some()
        );
    }

    #[test]
    fn read_only_view_matches_trimmed_view_without_mutating_samples() {
        let mut metrics = MetricsEngine::default();
        let now = Instant::now();
        metrics.samples.push_back(Sample {
            at: now - Duration::from_secs(16 * 60),
            cumulative_xp: 10_000,
            cumulative_gold: 500,
        });
        metrics.samples.push_back(Sample {
            at: now - Duration::from_secs(30),
            cumulative_xp: 10_120,
            cumulative_gold: 560,
        });

        let before_len = metrics.samples.len();
        let read_only = metrics.view_read_only(true);
        assert_eq!(metrics.samples.len(), before_len);

        let mut mutable_copy = metrics.clone();
        assert_eq!(read_only, mutable_copy.view(true));
        assert_eq!(metrics.samples.len(), before_len);
    }

    #[test]
    fn cumulative_view_handles_empty_windows_and_new_samples_after_clear() {
        let mut metrics = MetricsEngine::default();
        metrics.record(100, 40);
        metrics.on_battle_event(&BattleEvent::Unknown, false);
        assert_eq!(metrics.view_read_only(true).xp_per_hour, 0);

        metrics.record(25, 15);
        let view = metrics.view_read_only(true);
        assert!(view.xp_per_hour >= 25);
        assert!(view.gold_per_hour >= 15);
    }
}
