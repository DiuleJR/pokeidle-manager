use super::*;
use futures_util::{SinkExt, StreamExt};
use std::sync::{Arc, Mutex as StdMutex};
use tokio_tungstenite::accept_async;

const GAME_ORIGIN: &str = "https://pokeidle.io";
const SESSION_ID: &str = "restored-session";

#[test]
fn only_about_blank_page_uses_normal_blank_navigation_path() {
    assert_eq!(
        gated_startup_target_decision(true, 0),
        GatedStartupTargetDecision::UseBlankTarget
    );
}

#[test]
fn adopts_only_one_restored_top_level_game_page() {
    let one_game_page = vec![target("restored", "page", "https://pokeidle.io/app")];
    let one_game_count = one_game_page
        .iter()
        .filter(|target| is_game_page(target, GAME_ORIGIN))
        .count();
    assert_eq!(one_game_count, 1);
    assert_eq!(
        gated_startup_target_decision(false, one_game_count),
        GatedStartupTargetDecision::AdoptRestoredGameTarget
    );
    assert_eq!(
        gated_startup_target_decision(true, one_game_count),
        GatedStartupTargetDecision::AdoptRestoredGameTarget
    );
    let multiple_pages = vec![
        target("restored-a", "page", "https://pokeidle.io/app"),
        target("restored-b", "page", "https://pokeidle.io/market"),
    ];
    let game_count = multiple_pages
        .iter()
        .filter(|target| is_game_page(target, GAME_ORIGIN))
        .count();
    assert_eq!(game_count, 2);
    assert_eq!(
        gated_startup_target_decision(true, game_count),
        GatedStartupTargetDecision::BlockMultipleGameTargets,
        "a blank page must not make an ambiguous set of restored game pages adoptable"
    );
}

#[test]
fn restored_target_filter_ignores_iframes_and_challenge_pages() {
    let iframe = target("embedded-game", "iframe", "https://pokeidle.io/app");
    let challenge = target(
        "captcha",
        "page",
        "https://challenges.cloudflare.com/turnstile",
    );
    assert!(!is_game_page(&iframe, GAME_ORIGIN));
    assert!(!is_game_page(&challenge, GAME_ORIGIN));
    assert!(game_target_from_event(&target_event(&iframe), GAME_ORIGIN).is_none());
    assert!(game_target_from_event(&target_event(&challenge), GAME_ORIGIN).is_none());
}

#[test]
fn delayed_restored_game_target_event_is_detected_and_other_events_are_retained() {
    let mut pending = VecDeque::from([
        json!({
            "method": "Target.targetCreated",
            "params": {"targetInfo": {
                "targetId": "late-restored-game",
                "type": "page",
                "url": "https://pokeidle.io/app"
            }}
        }),
        json!({
            "method": "Target.targetCreated",
            "params": {"targetInfo": {
                "targetId": "late-iframe",
                "type": "iframe",
                "url": "https://pokeidle.io/app"
            }}
        }),
        json!({"method": "Runtime.executionContextCreated", "params": {}}),
    ]);

    let detected = take_game_target_events(&mut pending, GAME_ORIGIN);
    assert_eq!(
        detected
            .iter()
            .map(|target| target.id.as_str())
            .collect::<Vec<_>>(),
        vec!["late-restored-game"]
    );
    assert_eq!(pending.len(), 2);
    assert!(pending.iter().any(|event| {
        event
            .pointer("/params/targetInfo/targetId")
            .and_then(Value::as_str)
            == Some("late-iframe")
    }));
    assert!(pending.iter().any(|event| {
        event.get("method").and_then(Value::as_str) == Some("Runtime.executionContextCreated")
    }));
}

#[tokio::test]
async fn fresh_gated_startup_creates_one_manager_owned_blank_target_on_demand() {
    let (mut socket, server, requests) = fake_cdp(CdpReply::CreatedManagerTarget).await;
    let mut next_id = 31;
    let created = create_manager_blank_target(&mut socket, &mut next_id)
        .await
        .expect("the browser should return the one explicitly created bootstrap target");

    assert_eq!(created.id, "manager-owned-blank");
    assert_eq!(created.target_type, "page");
    assert_eq!(created.url, "about:blank");
    assert_eq!(next_id, 32);
    assert_eq!(requests.lock().unwrap()[0]["method"], "Target.createTarget");
    drop(socket);
    server.await.unwrap();
}

#[tokio::test]
async fn bootstrap_failure_cleanup_closes_only_the_manager_created_target() {
    let (mut socket, server, requests) = fake_cdp(CdpReply::ClosedManagerTarget).await;
    let mut next_id = 45;
    assert!(close_manager_created_target(&mut socket, &mut next_id, "manager-owned-blank").await);

    let requests = requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["method"], "Target.closeTarget");
    assert_eq!(requests[0]["params"]["targetId"], "manager-owned-blank");
    assert!(
        requests
            .iter()
            .all(|request| request["method"] != "Browser.close")
    );
    assert_eq!(next_id, 46);
    drop(socket);
    server.await.unwrap();
}

#[tokio::test]
async fn neutralization_consumes_navigation_events_flushed_before_command_response() {
    let (mut socket, server, requests) = fake_cdp(CdpReply::CommitAndLoad).await;
    let mut next_id = 41;
    let events = neutralize_restored_game_target(&mut socket, &mut next_id, SESSION_ID)
        .await
        .expect("a confirmed about:blank commit and load should pass the neutralization gate");

    assert_eq!(next_id, 42);
    assert!(events.iter().any(|event| {
        event.get("method").and_then(Value::as_str) == Some("Page.frameNavigated")
    }));
    assert!(events.iter().any(|event| {
        event.get("method").and_then(Value::as_str) == Some("Page.loadEventFired")
    }));
    let requests = requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["method"], "Page.navigate");
    assert_eq!(requests[0]["params"]["url"], "about:blank");
    assert_eq!(requests[0]["sessionId"], SESSION_ID);

    drop(socket);
    server.await.unwrap();
}

#[tokio::test]
async fn neutralization_cdp_error_fails_closed_without_close_or_followup_navigation() {
    let (mut socket, server, requests) = fake_cdp(CdpReply::RejectNavigation).await;
    let mut next_id = 7;
    let result = neutralize_restored_game_target(&mut socket, &mut next_id, SESSION_ID).await;

    assert!(result.is_err());
    assert_eq!(
        next_id, 7,
        "failed Page.navigate must not advance the CDP sequence"
    );
    let requests = requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["method"], "Page.navigate");
    assert!(requests.iter().all(|request| {
        !matches!(
            request.get("method").and_then(Value::as_str),
            Some("Browser.close" | "Target.closeTarget")
        )
    }));

    drop(socket);
    server.await.unwrap();
}

#[tokio::test]
async fn neutralized_baseline_requires_the_same_blank_target_and_no_other_game_page() {
    let (mut socket, server, requests) = fake_cdp(CdpReply::BaselineAboutBlank).await;
    let mut next_id = 51;
    let accepted =
        verify_neutralized_target_baseline(&mut socket, &mut next_id, "managed", GAME_ORIGIN)
            .await
            .expect("the adopted page at about:blank with no other game page is a valid baseline");
    assert_eq!(accepted.id, "managed");
    assert_eq!(accepted.url, "about:blank");
    assert_eq!(next_id, 52);
    assert_eq!(requests.lock().unwrap()[0]["method"], "Target.getTargets");
    drop(socket);
    server.await.unwrap();

    let (mut socket, server, requests) = fake_cdp(CdpReply::BaselineAdditionalGamePage).await;
    let mut next_id = 61;
    let rejected =
        verify_neutralized_target_baseline(&mut socket, &mut next_id, "managed", GAME_ORIGIN).await;
    assert!(rejected.is_err());
    assert_eq!(next_id, 62);
    assert_eq!(requests.lock().unwrap()[0]["method"], "Target.getTargets");
    drop(socket);
    server.await.unwrap();
}

#[test]
fn post_navigation_accepts_only_the_expected_unique_top_level_game_page() {
    let expected = target("managed", "page", "https://pokeidle.io/app");
    let iframe = target("embedded", "iframe", "https://pokeidle.io/app");
    let challenge = target("challenge", "page", "https://challenge.test/verify");
    assert_eq!(
        unique_game_target_after_navigation(
            &[expected.clone(), iframe, challenge],
            "managed",
            GAME_ORIGIN
        )
        .map(|target| target.id),
        Some("managed".into())
    );
    assert!(unique_game_target_after_navigation(&[], "managed", GAME_ORIGIN).is_none());
    assert!(
        unique_game_target_after_navigation(&[expected.clone()], "other", GAME_ORIGIN).is_none()
    );
    assert!(
        unique_game_target_after_navigation(
            &[
                expected,
                target("unexpected", "page", "https://pokeidle.io/other")
            ],
            "managed",
            GAME_ORIGIN
        )
        .is_none()
    );
}

#[tokio::test]
async fn bounded_pre_navigation_discovery_catches_a_late_target_event() {
    let (mut socket, server, requests) = fake_cdp(CdpReply::LateGameTarget).await;
    let mut next_id = 13;
    let found = find_unobserved_game_target_before_navigation(
        &mut socket,
        &mut next_id,
        "managed",
        GAME_ORIGIN,
    )
    .await
    .expect("late target discovery should complete without navigating");

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "late-game");
    assert!(
        requests
            .lock()
            .unwrap()
            .iter()
            .all(|request| request["method"] == "Target.getTargets")
    );
    drop(socket);
    server.await.unwrap();
}

#[tokio::test]
async fn post_navigation_verifier_rejects_a_late_second_game_target_event() {
    let (mut socket, server, requests) = fake_cdp(CdpReply::LateSecondTargetEvent).await;
    let mut next_id = 29;
    let result = verify_unique_game_target_after_navigation(
        &mut socket,
        &mut next_id,
        "managed",
        GAME_ORIGIN,
    )
    .await;

    assert!(result.is_err());
    let requests = requests.lock().unwrap().clone();
    assert_eq!(
        requests.len(),
        2,
        "the quiet window must poll again for late events"
    );
    assert_eq!(requests[0]["method"], "Target.getTargets");
    drop(socket);
    server.await.unwrap();
}

#[cfg(debug_assertions)]
#[test]
fn controlled_reload_preconditions_accept_exactly_one_observed_browser_socket() {
    let baseline = controlled_reload_precondition_decision(valid_controlled_reload_preconditions())
        .expect("controlled reload requires a fully covered Browser-owned baseline");
    assert_eq!(baseline.old_socket_id, "old-socket");
    assert_eq!(baseline.document_generation, 7);
}

#[cfg(debug_assertions)]
#[test]
fn controlled_reload_preconditions_reject_zero_or_multiple_sockets() {
    for socket_count in [0, 2] {
        let mut input = valid_controlled_reload_preconditions();
        input.observed_game_socket_count = socket_count;
        assert_eq!(
            controlled_reload_precondition_decision(input),
            Err(ControlledReloadBlockReason::SocketCountNotOne),
            "controlled reload must require exactly one pre-observed game socket"
        );
    }
}

#[cfg(debug_assertions)]
#[test]
fn controlled_reload_preconditions_reject_unobserved_game_targets() {
    let mut input = valid_controlled_reload_preconditions();
    input.unobserved_game_targets = 1;
    assert_eq!(
        controlled_reload_precondition_decision(input),
        Err(ControlledReloadBlockReason::UnobservedTargets)
    );
}

#[cfg(debug_assertions)]
#[test]
fn controlled_reload_preconditions_reject_an_active_rust_transport() {
    let mut input = valid_controlled_reload_preconditions();
    input.rust_ws_connected = true;
    assert_eq!(
        controlled_reload_precondition_decision(input),
        Err(ControlledReloadBlockReason::RustTransportConnected)
    );
}

#[cfg(debug_assertions)]
#[test]
fn controlled_reload_preconditions_reject_wrong_owner_modes() {
    let mut input = valid_controlled_reload_preconditions();
    input.interactive_owner_mode = false;
    assert_eq!(
        controlled_reload_precondition_decision(input),
        Err(ControlledReloadBlockReason::WrongSessionMode)
    );

    let mut input = valid_controlled_reload_preconditions();
    input.browser_owner = false;
    assert_eq!(
        controlled_reload_precondition_decision(input),
        Err(ControlledReloadBlockReason::BrowserOwnerNotOnline)
    );
}

#[cfg(debug_assertions)]
#[test]
fn controlled_reload_passes_only_after_old_close_new_generation_and_new_handshake() {
    let baseline = controlled_reload_precondition_decision(valid_controlled_reload_preconditions())
        .expect("valid baseline");
    let mut tracker = ControlledReloadTracker::new(baseline, SESSION_ID);

    tracker.observe(
        &controlled_event(
            "Network.webSocketClosed",
            json!({"requestId": "old-socket"}),
        ),
        GAME_ORIGIN,
    );
    tracker.observe(
        &controlled_event(
            "Page.frameNavigated",
            json!({"frame": {"id": "main", "url": "https://pokeidle.io/app"}}),
        ),
        GAME_ORIGIN,
    );
    tracker.observe(
        &controlled_event(
            "Network.webSocketCreated",
            json!({"requestId": "new-socket", "url": "wss://pokeidle.io/socket"}),
        ),
        GAME_ORIGIN,
    );
    tracker.observe(
        &controlled_event(
            "Network.webSocketHandshakeResponseReceived",
            json!({"requestId": "new-socket", "response": {"status": 101}}),
        ),
        GAME_ORIGIN,
    );

    assert_eq!(tracker.outcome(), ControlledReloadOutcome::Pass);
}

#[cfg(debug_assertions)]
#[test]
fn controlled_reload_fails_when_old_socket_has_a_frame_after_new_handshake() {
    let baseline = controlled_reload_precondition_decision(valid_controlled_reload_preconditions())
        .expect("valid baseline");
    let mut tracker = ControlledReloadTracker::new(baseline, SESSION_ID);

    tracker.observe(
        &controlled_event(
            "Page.frameNavigated",
            json!({"frame": {"id": "main", "url": "https://pokeidle.io/app"}}),
        ),
        GAME_ORIGIN,
    );
    tracker.observe(
        &controlled_event(
            "Network.webSocketCreated",
            json!({"requestId": "new-socket", "url": "wss://pokeidle.io/socket"}),
        ),
        GAME_ORIGIN,
    );
    tracker.observe(
        &controlled_event(
            "Network.webSocketHandshakeResponseReceived",
            json!({"requestId": "new-socket", "response": {"status": 101}}),
        ),
        GAME_ORIGIN,
    );
    tracker.observe(
        &controlled_event(
            "Network.webSocketFrameReceived",
            json!({"requestId": "old-socket", "response": {"opcode": 1}}),
        ),
        GAME_ORIGIN,
    );

    assert_eq!(tracker.outcome(), ControlledReloadOutcome::DualSocket);
}

#[cfg(debug_assertions)]
#[test]
fn controlled_reload_times_out_blocked_when_old_socket_close_is_absent() {
    let baseline = controlled_reload_precondition_decision(valid_controlled_reload_preconditions())
        .expect("valid baseline");
    let mut tracker = ControlledReloadTracker::new(baseline, SESSION_ID);
    tracker.observe(
        &controlled_event(
            "Page.frameNavigated",
            json!({"frame": {"id": "main", "url": "https://pokeidle.io/app"}}),
        ),
        GAME_ORIGIN,
    );
    tracker.observe(
        &controlled_event(
            "Network.webSocketCreated",
            json!({"requestId": "new-socket", "url": "wss://pokeidle.io/socket"}),
        ),
        GAME_ORIGIN,
    );
    tracker.observe(
        &controlled_event(
            "Network.webSocketHandshakeResponseReceived",
            json!({"requestId": "new-socket", "response": {"status": 101}}),
        ),
        GAME_ORIGIN,
    );

    assert_eq!(tracker.outcome(), ControlledReloadOutcome::Pending);
    assert_eq!(tracker.finish(), ControlledReloadOutcome::Blocked);
}

#[cfg(debug_assertions)]
#[test]
fn controlled_reload_times_out_blocked_when_no_close_or_new_handshake_arrives() {
    let baseline = controlled_reload_precondition_decision(valid_controlled_reload_preconditions())
        .expect("valid baseline");
    let tracker = ControlledReloadTracker::new(baseline, SESSION_ID);

    assert_eq!(tracker.outcome(), ControlledReloadOutcome::Pending);
    assert_eq!(tracker.finish(), ControlledReloadOutcome::Blocked);
}

#[cfg(debug_assertions)]
#[test]
fn controlled_reload_blocks_when_new_handshake_precedes_old_socket_close() {
    let baseline = controlled_reload_precondition_decision(valid_controlled_reload_preconditions())
        .expect("valid baseline");
    let mut tracker = ControlledReloadTracker::new(baseline, SESSION_ID);

    tracker.observe(
        &controlled_event(
            "Page.frameNavigated",
            json!({"frame": {"id": "main", "url": "https://pokeidle.io/app"}}),
        ),
        GAME_ORIGIN,
    );
    tracker.observe(
        &controlled_event(
            "Network.webSocketCreated",
            json!({"requestId": "new-socket", "url": "wss://pokeidle.io/socket"}),
        ),
        GAME_ORIGIN,
    );
    tracker.observe(
        &controlled_event(
            "Network.webSocketHandshakeResponse",
            json!({"requestId": "new-socket", "response": {"status": 101}}),
        ),
        GAME_ORIGIN,
    );
    tracker.observe(
        &controlled_event(
            "Network.webSocketClosed",
            json!({"requestId": "old-socket"}),
        ),
        GAME_ORIGIN,
    );

    assert_eq!(tracker.finish(), ControlledReloadOutcome::Blocked);
}

#[cfg(debug_assertions)]
fn valid_controlled_reload_preconditions() -> ControlledReloadPreconditions {
    ControlledReloadPreconditions {
        validation_mode_active: true,
        interactive_owner_mode: true,
        managed_target: true,
        network_covered_from_navigation: true,
        unobserved_game_targets: 0,
        game_page_count: 1,
        candidate_socket_id: Some("old-socket".into()),
        current_document_generation: 7,
        observed_game_socket_count: 1,
        candidate_socket: Some(ControlledReloadSocketSnapshot {
            socket_id: "old-socket".into(),
            document_generation: 7,
            handshake_response_received: true,
        }),
        browser_owner: true,
        account_online: true,
        rust_ws_connected: false,
        mutation_guard_active: true,
    }
}

#[cfg(debug_assertions)]
fn controlled_event(method: &str, params: Value) -> Value {
    json!({"method": method, "sessionId": SESSION_ID, "params": params})
}

#[test]
fn restored_target_failure_path_never_uses_global_browser_close() {
    let source = include_str!("browser.rs");
    let start = source
        .find("let mut bridge_reload_events = Vec::new();")
        .expect("restored-target neutralization block exists");
    let end = source[start..]
        .find("if bridge_plan.install_bridge {")
        .map(|offset| start + offset)
        .expect("normal gated startup follows restored-target handling");
    let restored_path = &source[start..end];
    let neutralize = restored_path
        .find("neutralize_restored_game_target(")
        .expect("restored target must be neutralized in place");
    let baseline = restored_path
        .find("verify_neutralized_target_baseline(")
        .expect("navigation requires a verified neutralized baseline");
    assert!(neutralize < baseline);
    assert!(restored_path.contains("ReloadGuardReason::UnobservedTarget"));
    assert!(!restored_path.contains("\"Browser.close\""));
    assert!(!restored_path.contains("\"Target.closeTarget\""));
}

fn target(id: &str, target_type: &str, url: &str) -> CdpTargetInfo {
    CdpTargetInfo {
        id: id.into(),
        target_type: target_type.into(),
        url: url.into(),
    }
}

fn target_event(target: &CdpTargetInfo) -> Value {
    json!({
        "method": "Target.targetCreated",
        "params": {"targetInfo": {
            "targetId": target.id,
            "type": target.target_type,
            "url": target.url
        }}
    })
}

#[derive(Clone, Copy)]
enum CdpReply {
    CommitAndLoad,
    RejectNavigation,
    CreatedManagerTarget,
    ClosedManagerTarget,
    LateGameTarget,
    LateSecondTargetEvent,
    BaselineAboutBlank,
    BaselineAdditionalGamePage,
}

async fn fake_cdp(
    reply: CdpReply,
) -> (
    CdpSocket,
    tokio::task::JoinHandle<()>,
    Arc<StdMutex<Vec<Value>>>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(StdMutex::new(Vec::new()));
    let server_requests = Arc::clone(&requests);
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        let mut request_number = 0;
        while let Some(Ok(Message::Text(raw))) = websocket.next().await {
            let request: Value = serde_json::from_str(&raw).unwrap();
            server_requests.lock().unwrap().push(request.clone());
            let id = request["id"].as_i64().unwrap();
            request_number += 1;

            match reply {
                CdpReply::CommitAndLoad => {
                    assert_eq!(request["method"], "Page.navigate");
                    // CDP can flush events before the command response. The
                    // client must preserve and consume these queued events.
                    for event in [
                        json!({
                            "method": "Page.frameNavigated",
                            "params": {"frame": {"id": "main", "url": "about:blank"}},
                            "sessionId": SESSION_ID
                        }),
                        json!({
                            "method": "Page.loadEventFired",
                            "params": {},
                            "sessionId": SESSION_ID
                        }),
                    ] {
                        websocket
                            .send(Message::Text(event.to_string().into()))
                            .await
                            .unwrap();
                    }
                    websocket
                        .send(Message::Text(
                            json!({"id": id, "result": {}}).to_string().into(),
                        ))
                        .await
                        .unwrap();
                    break;
                }
                CdpReply::RejectNavigation => {
                    assert_eq!(request["method"], "Page.navigate");
                    websocket
                        .send(Message::Text(
                            json!({
                                "id": id,
                                "error": {"code": -32000, "message": "navigation refused"}
                            })
                            .to_string()
                            .into(),
                        ))
                        .await
                        .unwrap();
                    break;
                }
                CdpReply::CreatedManagerTarget => {
                    assert_eq!(request["method"], "Target.createTarget");
                    assert_eq!(request["params"]["url"], "about:blank");
                    websocket
                        .send(Message::Text(
                            json!({"id": id, "result": {"targetId": "manager-owned-blank"}})
                                .to_string()
                                .into(),
                        ))
                        .await
                        .unwrap();
                    break;
                }
                CdpReply::ClosedManagerTarget => {
                    assert_eq!(request["method"], "Target.closeTarget");
                    assert_eq!(request["params"]["targetId"], "manager-owned-blank");
                    websocket
                        .send(Message::Text(
                            json!({"id": id, "result": {"success": true}})
                                .to_string()
                                .into(),
                        ))
                        .await
                        .unwrap();
                    break;
                }
                CdpReply::LateGameTarget => {
                    assert_eq!(request["method"], "Target.getTargets");
                    if request_number == 2 {
                        websocket
                            .send(Message::Text(
                                json!({
                                    "method": "Target.targetCreated",
                                    "params": {"targetInfo": {
                                        "targetId": "late-game",
                                        "type": "page",
                                        "url": "https://pokeidle.io/app"
                                    }}
                                })
                                .to_string()
                                .into(),
                            ))
                            .await
                            .unwrap();
                    }
                    let target_infos = if request_number == 1 {
                        json!([{"targetId":"managed","type":"page","url":"about:blank"}])
                    } else {
                        json!([{"targetId":"managed","type":"page","url":"about:blank"}])
                    };
                    websocket
                        .send(Message::Text(
                            json!({"id": id, "result": {"targetInfos": target_infos}})
                                .to_string()
                                .into(),
                        ))
                        .await
                        .unwrap();
                    if request_number == 2 {
                        break;
                    }
                }
                CdpReply::LateSecondTargetEvent => {
                    assert_eq!(request["method"], "Target.getTargets");
                    if request_number == 2 {
                        websocket
                            .send(Message::Text(
                                json!({
                                    "method": "Target.targetCreated",
                                    "params": {"targetInfo": {
                                        "targetId": "late-second",
                                        "type": "page",
                                        "url": "https://pokeidle.io/another"
                                    }}
                                })
                                .to_string()
                                .into(),
                            ))
                            .await
                            .unwrap();
                    }
                    websocket
                        .send(Message::Text(
                            json!({
                                "id": id,
                                "result": {"targetInfos": [
                                    {"targetId":"managed","type":"page","url":"https://pokeidle.io/app"}
                                ]}
                            })
                            .to_string()
                            .into(),
                        ))
                        .await
                        .unwrap();
                    if request_number == 2 {
                        break;
                    }
                }
                CdpReply::BaselineAboutBlank | CdpReply::BaselineAdditionalGamePage => {
                    assert_eq!(request["method"], "Target.getTargets");
                    let mut target_infos = vec![json!({
                        "targetId":"managed",
                        "type":"page",
                        "url":"about:blank"
                    })];
                    if matches!(reply, CdpReply::BaselineAdditionalGamePage) {
                        target_infos.push(json!({
                            "targetId":"unexpected-game",
                            "type":"page",
                            "url":"https://pokeidle.io/second"
                        }));
                    }
                    websocket
                        .send(Message::Text(
                            json!({"id": id, "result": {"targetInfos": target_infos}})
                                .to_string()
                                .into(),
                        ))
                        .await
                        .unwrap();
                    break;
                }
            }
        }
    });
    let (stream, _) = connect_async(format!("ws://{address}/devtools/browser/test"))
        .await
        .unwrap();
    (
        CdpSocket {
            stream,
            pending_events: VecDeque::new(),
        },
        server,
        requests,
    )
}
