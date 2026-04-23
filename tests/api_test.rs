use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use dashmap::DashMap;
use http_body_util::BodyExt;
use serde_json::json;
use serial_test::serial;
use shadow::replay::{tick_replay, ReplayState, ReplayStatus};
use shadow::{
    app, Ghost, GhostConfig, HeartbeatRequest, HeartbeatResponse, ServerState, Task, TaskStatus,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use tower::ServiceExt;

const API_PATH: &str = "/api/v1";
const GHOST_PATH: &str = "/ghost";
const CHARON_PATH: &str = "/charon";

enum Module {
    GHOST,
    CHARON,
}

fn api(module: Module, endpoint: String) -> String {
    if matches!(module, Module::GHOST) {
        format!("{}{}{}", API_PATH, GHOST_PATH, endpoint)
    } else {
        format!("{}{}{}", API_PATH, CHARON_PATH, endpoint)
    }
}

fn get_test_app() -> (axum::Router, Arc<ServerState>) {
    let state = Arc::new(ServerState {
        ghosts: DashMap::new(),
        pending_tasks: DashMap::new(),
        task_history: DashMap::new(),
        replay: RwLock::new(ReplayState::default()),
    });

    let replay_state = state.clone();
    tokio::spawn(async move {
        tick_replay(replay_state).await;
    });

    (app(state.clone()), state)
}

fn live_ghost(id: &str, hostname: &str, os: &str) -> Ghost {
    Ghost {
        id: id.to_string(),
        hostname: hostname.to_string(),
        os: os.to_string(),
        sysinfo: None,
        sleep_interval: None,
        jitter_percent: None,
        update_pending: None,
        last_seen: Some(0),
        is_replay: false,
    }
}

#[tokio::test]
async fn test_health_check() {
    let (app, _) = get_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"OK");
}

#[tokio::test]
async fn test_ghost_register_and_list() {
    let (app, state) = get_test_app();
    let ghost_id = "mock-uuid-54321";

    let payload = json!({
        "id": ghost_id,
        "hostname": "uwu-underground",
        "os": "linux",
        "last_seen": 0
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::GHOST, "/register".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(state.ghosts.contains_key(ghost_id));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(api(Module::CHARON, format!("/ghosts/{}", ghost_id)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ghost: Option<Ghost> = serde_json::from_slice(&body).unwrap();
    assert!(ghost.is_some());
    assert_eq!(ghost.as_ref().unwrap().id, ghost_id);
    assert!(!ghost.as_ref().unwrap().is_replay);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(api(Module::CHARON, "/ghosts".to_string()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let list: Vec<Ghost> = serde_json::from_slice(&body).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, ghost_id);
    assert!(!list[0].is_replay);
}

#[tokio::test]
async fn test_charon_get_unknown_ghost() {
    let (app, _) = get_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri(api(Module::CHARON, "/ghosts/unknown-id".to_string()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ghost: Option<Ghost> = serde_json::from_slice(&body).unwrap();
    assert!(ghost.is_none());
}

#[tokio::test]
async fn test_update_ghost_config_flow() {
    let (app, state) = get_test_app();
    let ghost_id = "config-ghost";

    state.ghosts.insert(
        ghost_id.to_string(),
        Ghost {
            id: ghost_id.to_string(),
            hostname: "test".to_string(),
            os: "linux".to_string(),
            sysinfo: None,
            sleep_interval: Some(5),
            jitter_percent: Some(1),
            update_pending: Some(false),
            last_seen: Some(0),
            is_replay: false,
        },
    );

    let config_payload = GhostConfig {
        sleep_interval: 60,
        jitter_percent: 10,
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, format!("/ghosts/{}", ghost_id)))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&config_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let ghost = state.ghosts.get(ghost_id).unwrap();
    assert_eq!(ghost.update_pending, Some(true));
    assert_eq!(ghost.sleep_interval, Some(60));
    assert_eq!(ghost.jitter_percent, Some(10));
    drop(ghost);

    let heartbeat_req = HeartbeatRequest {
        id: ghost_id.to_string(),
        results: None,
    };
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::GHOST, "/heartbeat".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&heartbeat_req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let hb_res: HeartbeatResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(hb_res.sleep_interval, 60);
    assert_eq!(hb_res.jitter_percent, 10);

    let ghost = state.ghosts.get(ghost_id).unwrap();
    assert_eq!(ghost.update_pending, Some(false));
}

#[tokio::test]
async fn test_update_unknown_ghost_config() {
    let (app, _) = get_test_app();
    let ghost_id = "unknown-ghost";

    let config_payload = GhostConfig {
        sleep_interval: 60,
        jitter_percent: 10,
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, format!("/ghosts/{}", ghost_id)))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&config_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_full_task_flow() {
    let (app, state) = get_test_app();
    let ghost_id = "active-ghost-1";

    state
        .ghosts
        .insert(ghost_id.to_string(), live_ghost(ghost_id, "test", "TempleOS"));

    let task_payload = json!({
        "command": "whoami",
        "args": ""
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, format!("/ghosts/{}/task", ghost_id)))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&task_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let tasks = state.pending_tasks.get(ghost_id).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, TaskStatus::Pending);

    let task_id = tasks[0].id.clone();
    drop(tasks);

    let heartbeat_req = HeartbeatRequest {
        id: ghost_id.to_string(),
        results: None,
    };
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::GHOST, "/heartbeat".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&heartbeat_req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let hb_res: HeartbeatResponse = serde_json::from_slice(&body).unwrap();
    assert!(hb_res.tasks.is_some());

    let received_tasks = hb_res.tasks.unwrap();
    assert_eq!(received_tasks[0].id, task_id);
    assert_eq!(received_tasks[0].command, "whoami");

    let tasks = state.pending_tasks.get(ghost_id).unwrap();
    assert_eq!(tasks[0].status, TaskStatus::Sent);
    drop(tasks);

    let result_payload = json!({
        "id": ghost_id,
        "results": [
            {
                "task_id": task_id,
                "status": "done",
                "output": "root"
            }
        ]
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::GHOST, "/heartbeat".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&result_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    if let Some(pending) = state.pending_tasks.get(ghost_id) {
        assert!(pending.is_empty(), "Task was not removed from pending list");
    }

    let history = state.task_history.get(ghost_id).unwrap();
    assert_eq!(history[0].status, TaskStatus::Done);
    assert_eq!(history[0].result, Some("root".to_string()));
}

#[tokio::test]
async fn test_kill_ghost() {
    let (app, state) = get_test_app();
    let ghost_id = "doomed-ghost";

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, format!("/ghosts/{}/kill", ghost_id)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let tasks = state.pending_tasks.get(ghost_id).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].command, "STOP_HAUNT");
}

#[tokio::test]
async fn test_heartbeat_unknown_ghost() {
    let (app, _) = get_test_app();

    let heartbeat_req = HeartbeatRequest {
        id: "unknown".to_string(),
        results: None,
    };
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::GHOST, "/heartbeat".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&heartbeat_req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_heartbeat_no_outgoing_tasks() {
    let (app, state) = get_test_app();
    let ghost_id = "idle-ghost-51240";

    state
        .ghosts
        .insert(ghost_id.to_string(), live_ghost(ghost_id, "test", "linux"));

    let heartbeat_req = HeartbeatRequest {
        id: ghost_id.to_string(),
        results: None,
    };
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::GHOST, "/heartbeat".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&heartbeat_req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let hb_res: HeartbeatResponse = serde_json::from_slice(&body).unwrap();
    assert!(hb_res.tasks.is_none());
}

#[tokio::test]
async fn test_charon_get_ghost_tasks_combined() {
    let (app, state) = get_test_app();
    let ghost_id = "history-ghost-321";

    let pending_task = Task {
        id: "pending-task-id".to_string(),
        command: "whoami".to_string(),
        args: "".to_string(),
        status: TaskStatus::Pending,
        result: None,
    };
    state
        .pending_tasks
        .insert(ghost_id.to_string(), vec![pending_task]);

    let history_task = Task {
        id: "historical-task-id".to_string(),
        command: "whoami".to_string(),
        args: "".to_string(),
        status: TaskStatus::Done,
        result: Some("root".to_string()),
    };
    state
        .task_history
        .insert(ghost_id.to_string(), vec![history_task]);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(api(Module::CHARON, format!("/ghosts/{}/tasks", ghost_id)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let tasks: Vec<Task> = serde_json::from_slice(&body).unwrap();

    assert_eq!(tasks.len(), 2);
    assert!(tasks.iter().any(|t| t.id == "pending-task-id"));
    assert!(tasks.iter().any(|t| t.id == "historical-task-id"));
}

#[tokio::test]
async fn test_charon_get_task_details() {
    let (app, state) = get_test_app();
    let ghost_id = "detail-ghost";

    let history_task = Task {
        id: "historical-task-id".to_string(),
        command: "ls".to_string(),
        args: "-la".to_string(),
        status: TaskStatus::Done,
        result: Some("total 0".to_string()),
    };
    state
        .task_history
        .insert(ghost_id.to_string(), vec![history_task]);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(api(Module::CHARON, "/tasks/historical-task-id".to_string()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let task: Option<Task> = serde_json::from_slice(&body).unwrap();
    assert!(task.is_some());
    assert_eq!(task.unwrap().result, Some("total 0".to_string()));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(api(Module::CHARON, "/tasks/non-existent-id".to_string()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let task: Option<Task> = serde_json::from_slice(&body).unwrap();
    assert!(task.is_none());
}

#[tokio::test]
async fn test_charon_get_pending_task_details() {
    let (app, state) = get_test_app();
    let ghost_id = "pending-detail-ghost";

    let pending_task = Task {
        id: "pending-task-id".to_string(),
        command: "echo".to_string(),
        args: "hello".to_string(),
        status: TaskStatus::Pending,
        result: None,
    };
    state
        .pending_tasks
        .insert(ghost_id.to_string(), vec![pending_task]);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(api(Module::CHARON, "/tasks/pending-task-id".to_string()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let task: Option<Task> = serde_json::from_slice(&body).unwrap();
    assert!(task.is_some());
    assert_eq!(task.unwrap().status, TaskStatus::Pending);
}

#[tokio::test]
#[serial]
async fn test_replay_status_defaults_to_stopped() {
    let (app, _) = get_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(api(Module::CHARON, "/replay".to_string()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let status: ReplayStatus = serde_json::from_slice(&body).unwrap();

    assert!(!status.running);
    assert!(status.current_scenario.is_none());
    assert_eq!(status.replay_ghost_count, 0);
    assert_eq!(
        status.available_scenarios,
        vec![
            "idle_fleet".to_string(),
            "task_flow".to_string(),
            "loot_burst".to_string()
        ]
    );
}

#[tokio::test]
#[serial]
async fn test_replay_start_idle_fleet_registers_replay_ghosts() {
    let (app, state) = get_test_app();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, "/replay/start".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({ "scenario": "idle_fleet" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let status: ReplayStatus = serde_json::from_slice(&body).unwrap();
    assert!(status.running);
    assert_eq!(status.current_scenario.as_deref(), Some("idle_fleet"));
    assert_eq!(status.replay_ghost_count, 3);

    assert_eq!(state.ghosts.len(), 3);
    assert!(state.ghosts.iter().all(|entry| entry.value().is_replay));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(api(Module::CHARON, "/ghosts".to_string()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ghosts: Vec<Ghost> = serde_json::from_slice(&body).unwrap();
    assert_eq!(ghosts.len(), 3);
    assert!(ghosts.iter().all(|ghost| ghost.is_replay));
}

#[tokio::test]
#[serial]
async fn test_replay_start_invalid_scenario_returns_bad_request() {
    let (app, _) = get_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, "/replay/start".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({ "scenario": "definitely_not_real" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[serial]
async fn test_replay_task_flow_completes_queued_exec() {
    let (app, state) = get_test_app();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, "/replay/start".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({ "scenario": "task_flow" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, "/ghosts/replay-task-01/task".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "command": "EXEC",
                        "args": "hostname"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    sleep(Duration::from_secs(4)).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(api(
                    Module::CHARON,
                    "/ghosts/replay-task-01/tasks".to_string(),
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let tasks: Vec<Task> = serde_json::from_slice(&body).unwrap();

    let queued_task = tasks
        .iter()
        .find(|task| task.command == "EXEC" && task.args == "hostname")
        .unwrap();

    assert_eq!(queued_task.status, TaskStatus::Done);
    assert_eq!(queued_task.result.as_deref(), Some("jumpbox-01"));

    if let Some(pending) = state.pending_tasks.get("replay-task-01") {
        assert!(pending.iter().all(|task| task.args != "hostname"));
    }
}

#[tokio::test]
#[serial]
async fn test_replay_config_update_applies_and_clears_pending() {
    let (app, state) = get_test_app();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, "/replay/start".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({ "scenario": "idle_fleet" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let config_payload = GhostConfig {
        sleep_interval: 45,
        jitter_percent: 9,
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, "/ghosts/replay-idle-01".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&config_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    {
        let ghost = state.ghosts.get("replay-idle-01").unwrap();
        assert_eq!(ghost.sleep_interval, Some(45));
        assert_eq!(ghost.jitter_percent, Some(9));
        assert_eq!(ghost.update_pending, Some(true));
    }

    sleep(Duration::from_secs(3)).await;

    let ghost = state.ghosts.get("replay-idle-01").unwrap();
    assert_eq!(ghost.sleep_interval, Some(45));
    assert_eq!(ghost.jitter_percent, Some(9));
    assert_eq!(ghost.update_pending, Some(false));
}

#[tokio::test]
#[serial]
async fn test_replay_reset_preserves_live_ghosts() {
    let (app, state) = get_test_app();
    let live_id = "live-ghost-01";

    state
        .ghosts
        .insert(live_id.to_string(), live_ghost(live_id, "real-host", "linux"));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, "/replay/start".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({ "scenario": "idle_fleet" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(state.ghosts.len(), 4);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, "/replay/reset".to_string()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let status: ReplayStatus = serde_json::from_slice(&body).unwrap();

    assert!(!status.running);
    assert!(status.current_scenario.is_none());
    assert_eq!(status.replay_ghost_count, 0);

    assert_eq!(state.ghosts.len(), 1);
    let live = state.ghosts.get(live_id).unwrap();
    assert!(!live.is_replay);
    assert_eq!(live.hostname, "real-host");
}

#[tokio::test]
#[serial]
async fn test_replay_loot_burst_generates_and_reset_cleans_replay_files() {
    let (app, _) = get_test_app();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, "/replay/start".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({ "scenario": "loot_burst" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    sleep(Duration::from_secs(5)).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(api(Module::CHARON, "/loot".to_string()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let files: Vec<String> = serde_json::from_slice(&body).unwrap();
    assert!(files.iter().any(|file| file.starts_with("replay_")));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, "/replay/reset".to_string()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(api(Module::CHARON, "/loot".to_string()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let files: Vec<String> = serde_json::from_slice(&body).unwrap();
    assert!(!files.iter().any(|file| file.starts_with("replay_")));
}
