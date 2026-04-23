use dashmap::DashMap;
use std::sync::Arc;
use std::net::SocketAddr;
use shadow::{app, ServerState};
use std::env;
use shadow::replay::{tick_replay, ReplayState};
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    // init shared state
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

    let app_router = app(state);

    let url = env::var("SHADOW_URL").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("SHADOW_PORT").unwrap_or_else(|_| "9999".to_string());
    let addr_str = format!("{}:{}", url, port);
    let addr: SocketAddr = addr_str.parse().expect("Invalid address format");
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    
    println!("SHADOW listening on {}", addr);
    axum::serve(listener, app_router).await.unwrap();
}
