use dashmap::DashMap;
use std::sync::Arc;
use std::net::SocketAddr;
use SHADOW::{app, ServerState};

#[tokio::main]
async fn main() {
    // init shared state
    let state = Arc::new(ServerState {
        ghosts: DashMap::new(),
        tasks: DashMap::new()
    });

    let app_router = app(state);

    // start listening
    let addr = SocketAddr::from(([0, 0, 0, 0], 9999));  // TODO: adress and port configuration via CHARON (NOTE: is this still necessary in Docker? or maybe a separate mechanism would suffice?)
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    
    println!("SHADOW listening on {}", addr);
    axum::serve(listener, app_router).await.unwrap();
}
