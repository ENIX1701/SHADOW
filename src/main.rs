use dashmap::DashMap;
use std::sync::Arc;
use std::net::SocketAddr;
use shadow::{app, ServerState};
use std::env;

#[tokio::main]
async fn main() {
    // init shared state
    let state = Arc::new(ServerState {
        ghosts: DashMap::new(),
        pending_tasks: DashMap::new(),
        task_history: DashMap::new()
    });

    let app_router = app(state);

    // start listening
    let port = env::var("SHADOW_PORT").unwrap_or_else(|_| "9999".to_string());
    let addr_str = format!("0.0.0.0:{}", port);
    let addr: SocketAddr = addr_str.parse().expect("Invalid address format");
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    
    println!("SHADOW listening on {}", addr);
    axum::serve(listener, app_router).await.unwrap();
}
