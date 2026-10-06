use crate::{
    accounts::{AccountManager, new_record},
    protocol::ServerFrame,
};
pub fn seed_four_accounts(manager: &AccountManager) {
    for (index, nick) in [
        "DemoTrainerTwo",
        "DemoTrainerThree",
        "DemoTrainerFour",
        "DemoTrainerFive",
    ]
    .iter()
    .enumerate()
    {
        let id = format!("mock-{}", index + 1);
        if manager.count() >= 4 {
            return;
        }
        if manager
            .add(new_record(
                id.clone(),
                (*nick).into(),
                ["#62d4b4", "#8b9dff", "#f2b86a", "#e589b4"][index].into(),
            ))
            .is_ok()
        {
            let raw = format!(
                r#"{{"t":"welcome","estado":{{"nick":"{}","level":{},"xp":1000,"gold":10000,"diamonds":0,"orbs":0,"huntSlug":"demo_route","noCentro":false,"items":{{"204":120}},"balls":{{"4":300}},"pokemons":[{{"id":1,"nome":"DemoMon","level":{},"hp":90,"maxHp":100}}]}}}}"#,
                nick,
                50 + index as u32,
                50 + index as u32
            );
            let _ = manager.ingest(&id, ServerFrame::parse(&raw).expect("static mock frame"));
        }
    }
}

/// Emits deterministic battle frames through the exact production ingestion path.
/// No view metric is written directly: `AccountRuntime::ingest` feeds EventBus and
/// MetricsEngine, just like a frame received from the game would.
pub fn advance(manager: &AccountManager, tick: u64) {
    for (index, account_id) in ["mock-1", "mock-2"].iter().enumerate() {
        let xp = 1_800 + ((tick + index as u64 * 3) % 7) * 113;
        let gold = 330 + ((tick + index as u64) % 5) * 31;
        let raw = format!(
            r#"{{"t":"batalha","ev":[{{"k":"morte","quem":"selvagem","slot":1,"xpTreinador":{xp},"ouro":{gold},"ouroVendaAuto":0}}]}}"#,
        );
        if let Ok(frame) = ServerFrame::parse(&raw) {
            let _ = manager.ingest(account_id, frame);
        }
    }
}
pub fn browser_mode(manager: &AccountManager, id: &str) {
    if let Some(snapshot) = manager
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.account.id == id)
    {
        let _ = snapshot;
    } /* Mode transitions are driven by browser runtime in real mode. */
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_events_use_the_account_metrics_pipeline() {
        let manager = AccountManager::default();
        seed_four_accounts(&manager);
        advance(&manager, 1);
        let snapshots = manager.snapshots();
        let first = snapshots
            .iter()
            .find(|snapshot| snapshot.account.id == "mock-1")
            .unwrap();
        assert_eq!(first.metrics.kills, 1);
        assert!(first.metrics.xp_per_hour > 0);
    }
}
