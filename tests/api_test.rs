use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use dashmap::DashMap;
use http_body_util::BodyExt;
use serde_json::json;
use shadow::{app, Ghost, GhostConfig, HeartbeatRequest, HeartbeatResponse, ServerState, Task, TaskStatus};
use std::sync::Arc;
use tower::ServiceExt;

// setup for future parametrization
const API_PATH: &str = "/api/v1";
const GHOST_PATH: &str = "/ghost";
const CHARON_PATH: &str = "/charon";

enum Module {
    GHOST,
    CHARON
}

fn api(module: Module, endpoint: String) -> String {
    if matches!(module, Module::GHOST) {
        format!("{}{}{}", API_PATH, GHOST_PATH, endpoint)
    } else if matches!(module, Module::CHARON) {
        format!("{}{}{}", API_PATH, CHARON_PATH, endpoint)
    } else {
        format!("{}{}", API_PATH, endpoint)
    }
}

// fresh state for each test
fn get_test_app() -> (axum::Router, Arc<ServerState>) {
    let state = Arc::new(ServerState {
        ghosts: DashMap::new(),
        pending_tasks: DashMap::new(),
        task_history: DashMap::new()
    });

    (app(state.clone()), state)
}

#[tokio::test]
async fn test_health_check() {
    let (app, _) = get_test_app();

    let response = app
        .oneshot(Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap())
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

    let response = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::GHOST, "/register".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap()
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(state.ghosts.contains_key(ghost_id));

    let response = app.clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(api(Module::CHARON, format!("/ghosts/{}", ghost_id)))
                .body(Body::empty())
                .unwrap()
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ghost: Option<Ghost> = serde_json::from_slice(&body).unwrap();
    assert!(ghost.is_some());
    assert_eq!(ghost.unwrap().id, ghost_id);

    let response = app.clone()
        .oneshot(
            Request::builder()
            .method("GET")
            .uri(api(Module::CHARON, "/ghosts".to_string()))
            .body(Body::empty())
            .unwrap()
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let list: Vec<Ghost> = serde_json::from_slice(&body).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, ghost_id);
}

#[tokio::test]
async fn test_charon_get_unknown_ghost() {
    let (app, _) = get_test_app();

    let response = app
        .oneshot(Request::builder()
            .uri(api(Module::CHARON, "/ghosts/unknown-id".to_string()))
            .body(Body::empty())
            .unwrap())
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

    state.ghosts.insert(ghost_id.to_string(), Ghost {
        id: ghost_id.to_string(),
        hostname: "test".to_string(),
        os: "linux".to_string(),
        sleep_interval: Some(5),
        jitter_percent: Some(1),
        update_pending: Some(false),
        last_seen: None
    });

    let config_payload = GhostConfig { 
        sleep_interval: 60,
        jitter_percent: 10
    };

    let response = app.clone()
        .oneshot(Request::builder()
            .method("POST")
            .uri(api(Module::CHARON, format!("/ghosts/{}", ghost_id)))
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string(&config_payload).unwrap()))
            .unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let ghost = state.ghosts.get(ghost_id).unwrap();
    assert_eq!(ghost.update_pending, Some(true));
    assert_eq!(ghost.sleep_interval, Some(60));
    assert_eq!(ghost.jitter_percent, Some(10));
    drop(ghost);

    let heartbeat_req = HeartbeatRequest { id: ghost_id.to_string(), results: None };
    let response = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::GHOST, "/heartbeat".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&heartbeat_req).unwrap()))
                .unwrap()
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
        jitter_percent: 10
    };

    let response = app.clone()
        .oneshot(Request::builder()
            .method("POST")
            .uri(api(Module::CHARON, format!("/ghosts/{}", ghost_id)))
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string(&config_payload).unwrap()))
            .unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_full_task_flow() {
    let (app, state) = get_test_app();
    let ghost_id = "active-ghost-1";

    state.ghosts.insert(ghost_id.to_string(), Ghost {
        id: ghost_id.to_string(),
        hostname: "test".to_string(),
        os: "TempleOS".to_string(),
        sleep_interval: None,
        jitter_percent: None,
        update_pending: None,
        last_seen: None
    });

    let task_payload = json!({
        "command": "whoami",
        "args": "".to_string()
    });

    let response = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, format!("/ghosts/{}/task", ghost_id)))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&task_payload).unwrap()))
                .unwrap()
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let tasks = state.pending_tasks.get(ghost_id).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, TaskStatus::Pending);

    let task_id = tasks[0].id.clone();
    drop(tasks);

    let heartbeat_req = HeartbeatRequest { id: ghost_id.to_string(), results: None };
    let response = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::GHOST, "/heartbeat".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&heartbeat_req).unwrap()))
                .unwrap()
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

    let response = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::GHOST, "/heartbeat".to_string()))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&result_payload).unwrap()))
                .unwrap()
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

    let response = app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(api(Module::CHARON, format!("/ghosts/{}/kill", ghost_id)))
                .body(Body::empty())
                .unwrap()
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

    let heartbeat_req = HeartbeatRequest { id: "unknown".to_string(), results: None };
    let response = app
        .oneshot(Request::builder()
            .method("POST")
            .uri(api(Module::GHOST, "/heartbeat".to_string()))
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string(&heartbeat_req).unwrap()))
            .unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_heartbeat_no_outgoing_tasks() {
    let (app, state) = get_test_app();
    let ghost_id = "idle-ghost-51240";

    state.ghosts.insert(ghost_id.to_string(), Ghost {
        id: ghost_id.to_string(),
        hostname: "test".to_string(),
        os: "linux".to_string(),
        sleep_interval: None,
        jitter_percent: None,
        update_pending: None,
        last_seen: None
    });

    let heartbeat_req = HeartbeatRequest { id: ghost_id.to_string(), results: None };
    let response = app
        .oneshot(Request::builder()
            .method("POST")
            .uri(api(Module::GHOST, "/heartbeat".to_string()))
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string(&heartbeat_req).unwrap()))
            .unwrap())
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
        result: None
    };
    state.pending_tasks.insert(ghost_id.to_string(), vec![pending_task]);

    let history_task = Task {
        id: "historical-task-id".to_string(),
        command: "whoami".to_string(),
        args: "".to_string(),
        status: TaskStatus::Done,
        result: Some("root".to_string())
    };
    state.task_history.insert(ghost_id.to_string(), vec![history_task]);

    let response = app
        .oneshot(Request::builder()
            .method("GET")
            .uri(api(Module::CHARON, format!("/ghosts/{}/tasks", ghost_id)))
            .body(Body::empty())
            .unwrap())
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
        result: Some("total 0".to_string())
    };
    state.task_history.insert(ghost_id.to_string(), vec![history_task]);

    let response = app.clone()
        .oneshot(Request::builder()
            .method("GET")
            .uri(api(Module::CHARON, format!("/tasks/{}", "historical-task-id".to_string())))
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let task: Option<Task> = serde_json::from_slice(&body).unwrap();
    assert!(task.is_some());
    assert_eq!(task.unwrap().result, Some("total 0".to_string()));

    let response = app.clone()
        .oneshot(Request::builder()
            .method("GET")
            .uri(api(Module::CHARON, format!("/tasks/{}", "non-existent-id".to_string())))
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let task: Option<Task> = serde_json::from_slice(&body).unwrap();
    assert!(task.is_none());
}

#[tokio::test]
async fn test_charon_get_pending_task_details() {

}

// TODO: leave for now, will not pass once this is implemented, which is what I want
#[tokio::test]
#[should_panic(expected = "ghost exfiltration todo")]
async fn test_ghost_upload_panic() {
    let (app, _) = get_test_app();

    let _ = app
        .oneshot(Request::builder()
            .method("POST")
            .uri(api(Module::GHOST, "/upload".to_string()))
            .body(Body::empty())
            .unwrap())
        .await;
}

#[tokio::test]
async fn test_charon_build() {
    let (app, _) = get_test_app();

    let build_req = json!({
        "target_url": "127.0.0.1",
        "target_port": "9999",
        "enable_debug": true,
        "enable_persistence": false,
        "persist_runcontrol": false,
        "persist_service": false,
        "persist_cron": false,
        "enable_impact": false,
        "impact_encrypt": false,
        "impact_wipe": false,
        "enable_exfil": true,
        "exfil_http": true,
        "exfil_dns": false,
    });

    let response = app
        .oneshot(Request::builder()
            .method("POST")
            .uri(api(Module::CHARON, format!("/build")))
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string(&build_req).unwrap()))
            .unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
