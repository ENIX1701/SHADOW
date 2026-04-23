use crate::{Ghost, ServerState, Task, TaskStatus};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::time::{Duration, sleep};

// okay a huge disclaimer for anyone reading this code
// this is being put together by duct tape and hope alone
// it's a proof-of-concept more than a prod-ready thing
// i'm so sorry QwQ

const REPLAY_TICK_SECONDS: u64 = 1;
const REPLAY_LOOT_PREFIX: &str = "replay_";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayStatus {
    pub running: bool,
    pub current_scenario: Option<String>,
    pub available_scenarios: Vec<String>,
    pub replay_ghost_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReplayStartRequest {
    pub scenario: String,
}

#[derive(Debug, Clone, Default)]
pub struct ReplayState {
    pub running: bool,
    pub current_scenario: Option<String>,
    pub replay_ghost_ids: HashSet<String>,
    pub scheduled_results: Vec<ScheduledReplayResult>,
}

#[derive(Debug, Clone)]
pub struct ScheduledReplayResult {
    pub ghost_id: String,
    pub task_id: String,
    pub command: String,
    pub args: String,
    pub ready_at: i64,
    pub status: TaskStatus,
    pub output: String,
    pub loot_filename: Option<String>,
    pub loot_contents: Option<String>,
    pub remove_ghost: bool,
    pub from_pending: bool,
}

#[derive(Debug, Clone, Copy)]
struct ReplayGhostSeed {
    id: &'static str,
    hostname: &'static str,
    os: &'static str,
    sleep_interval: i64,
    jitter_percent: i16,
}

// let's keep these "scenarios" as generic as possible for now
pub fn available_scenarios() -> Vec<String> {
    vec![
        "idle_fleet".to_string(),
        "task_flow".to_string(),
        "loot_burst".to_string(),
    ]
}

pub async fn build_replay_status(state: Arc<ServerState>) -> ReplayStatus {
    let replay = state.replay.read().await;
    status_from_state(&replay)
}

pub async fn start_replay(state: Arc<ServerState>, scenario: &str) -> Result<ReplayStatus, String> {
    if !available_scenarios()
        .iter()
        .any(|candidate| candidate == scenario)
    {
        return Err(format!("unknown replay scenario '{}'", scenario));
    }

    let mut replay = state.replay.write().await;
    reset_replay_locked(&state, &mut replay).await?;
    seed_scenario(&state, &mut replay, scenario);

    replay.running = true;
    replay.current_scenario = Some(scenario.to_string());

    Ok(status_from_state(&replay))
}

pub async fn stop_replay(state: Arc<ServerState>) -> Result<ReplayStatus, String> {
    let mut replay = state.replay.write().await;
    replay.running = false;
    Ok(status_from_state(&replay))
}

pub async fn reset_replay(state: Arc<ServerState>) -> Result<ReplayStatus, String> {
    let mut replay = state.replay.write().await;
    reset_replay_locked(&state, &mut replay).await?;
    Ok(status_from_state(&replay))
}

pub async fn tick_replay(state: Arc<ServerState>) {
    loop {
        sleep(Duration::from_secs(REPLAY_TICK_SECONDS)).await;

        let due = {
            let mut replay = state.replay.write().await;
            if !replay.running {
                continue;
            }

            let now = Utc::now().timestamp();
            refresh_replay_ghosts(&state, &replay.replay_ghost_ids, now);
            schedule_pending_results(&state, &mut replay, now);

            let mut due = Vec::new();
            replay.scheduled_results.retain(|entry| {
                if entry.ready_at <= now {
                    due.push(entry.clone());
                    false
                } else {
                    true
                }
            });

            due
        };

        for scheduled in due {
            complete_scheduled_result(state.clone(), scheduled).await;
        }
    }
}

fn status_from_state(replay: &ReplayState) -> ReplayStatus {
    ReplayStatus {
        running: replay.running,
        current_scenario: replay.current_scenario.clone(),
        available_scenarios: available_scenarios(),
        replay_ghost_count: replay.replay_ghost_ids.len(),
    }
}

// each ghost will use a regular ticker like a good boy
// need to re-verify after the PoC tho
fn refresh_replay_ghosts(state: &Arc<ServerState>, ghost_ids: &HashSet<String>, now: i64) {
    for ghost_id in ghost_ids {
        if let Some(mut ghost) = state.ghosts.get_mut(ghost_id) {
            ghost.last_seen = Some(now);

            if ghost.update_pending == Some(true) {
                ghost.update_pending = Some(false);
            }
        }
    }
}

fn schedule_pending_results(state: &Arc<ServerState>, replay: &mut ReplayState, now: i64) {
    let replay_ids: Vec<String> = replay.replay_ghost_ids.iter().cloned().collect();

    for ghost_id in replay_ids {
        let Some(ghost) = state.ghosts.get(&ghost_id) else {
            continue;
        };

        let hostname = ghost.hostname.clone();
        drop(ghost);

        if let Some(mut tasks) = state.pending_tasks.get_mut(&ghost_id) {
            for task in tasks
                .iter_mut()
                .filter(|task| task.status == TaskStatus::Pending)
            {
                if replay
                    .scheduled_results
                    .iter()
                    .any(|entry| entry.task_id == task.id)
                {
                    continue;
                }

                task.status = TaskStatus::Sent;
                replay
                    .scheduled_results
                    .push(build_scheduled_result(&ghost_id, &hostname, task, now));
            }
        }
    }
}

fn build_scheduled_result(
    ghost_id: &str,
    hostname: &str,
    task: &Task,
    now: i64,
) -> ScheduledReplayResult {
    let (output, loot_filename, loot_contents, remove_ghost) = match task.command.as_str() {
        "EXEC" => (build_exec_output(hostname, &task.args), None, None, false),
        "IMPACT" => (
            "replay mode: impact simulated; no destructive action executed >w<".to_string(),
            None,
            None,
            false,
        ),
        "EXFIL" => {
            let filename = format!("{}{}_{}.txt", REPLAY_LOOT_PREFIX, hostname, now);
            let contents = format!(
                "replay loot from {}\nsource=manual_exfil\ncaptured_at={}\n",
                hostname, now
            );

            (
                format!("replay mode: synthetic loot generated :3 -> {}", filename),
                Some(filename),
                Some(contents),
                false,
            )
        }
        "STOP_HAUNT" => (
            "replay mode: ghost shutdown simulated @w@".to_string(),
            None,
            None,
            true,
        ),
        other => (
            format!("replay mode: unsupported command '{}' acknowledged", other),
            None,
            None,
            false,
        ),
    };

    ScheduledReplayResult {
        ghost_id: ghost_id.to_string(),
        task_id: task.id.clone(),
        command: task.command.clone(),
        args: task.args.clone(),
        ready_at: now + 2,
        status: TaskStatus::Done,
        output,
        loot_filename,
        loot_contents,
        remove_ghost,
        from_pending: true,
    }
}

fn build_exec_output(hostname: &str, args: &str) -> String {
    let normalized = args.trim().to_lowercase();

    // okay so
    // this is mocked
    // fully
    // wiring this to regular ghosts (and spawning them...) would be difficult
    // it *may* happen in the future if PoC gets good feedback
    match normalized.as_str() {
        "whoami" => "replay-operator".to_string(),
        "hostname" => hostname.to_string(),
        "pwd" => "/home/replay/demo".to_string(),
        "uname -a" => format!(
            "Linux {} 6.8.0-replay #1 SMP PREEMPT_DYNAMIC x86_64 GNU/Linux",
            hostname
        ),
        cmd if cmd == "ls" || cmd == "ls -la" => {
            "Documents\nDownloads\nnotes.txt\ntelemetry.log".to_string()
        }
        _ => format!("replay mode: simulated EXEC result for '{}'", args.trim()),
    }
}

async fn complete_scheduled_result(state: Arc<ServerState>, scheduled: ScheduledReplayResult) {
    let mut completed_task = None;

    if scheduled.from_pending {
        if let Some(mut pending) = state.pending_tasks.get_mut(&scheduled.ghost_id) {
            if let Some(index) = pending.iter().position(|task| task.id == scheduled.task_id) {
                let mut task = pending.remove(index);
                task.status = scheduled.status.clone();
                task.result = Some(scheduled.output.clone());
                completed_task = Some(task);
            }
        }
    }

    let task = completed_task.unwrap_or_else(|| Task {
        id: scheduled.task_id.clone(),
        command: scheduled.command.clone(),
        args: scheduled.args.clone(),
        status: scheduled.status.clone(),
        result: Some(scheduled.output.clone()),
    });

    state
        .task_history
        .entry(scheduled.ghost_id.clone())
        .or_insert_with(Vec::new)
        .push(task);

    if let (Some(filename), Some(contents)) = (
        scheduled.loot_filename.as_deref(),
        scheduled.loot_contents.as_deref(),
    ) {
        if let Err(error) = write_replay_loot(filename, contents).await {
            eprintln!("failed to write replay loot :c ['{}': {}]", filename, error);
        }
    }

    if scheduled.remove_ghost {
        remove_single_replay_ghost(state, &scheduled.ghost_id).await;
    }
}

async fn remove_single_replay_ghost(state: Arc<ServerState>, ghost_id: &str) {
    state.ghosts.remove(ghost_id);
    state.pending_tasks.remove(ghost_id);

    let mut replay = state.replay.write().await;
    replay.replay_ghost_ids.remove(ghost_id);
    replay
        .scheduled_results
        .retain(|entry| entry.ghost_id != ghost_id);
}

async fn reset_replay_locked(
    state: &Arc<ServerState>,
    replay: &mut ReplayState,
) -> Result<(), String> {
    let replay_ids: Vec<String> = replay.replay_ghost_ids.iter().cloned().collect();

    for ghost_id in replay_ids {
        state.ghosts.remove(&ghost_id);
        state.pending_tasks.remove(&ghost_id);
        state.task_history.remove(&ghost_id);
    }

    replay.running = false;
    replay.current_scenario = None;
    replay.replay_ghost_ids.clear();
    replay.scheduled_results.clear();

    remove_replay_loot_files().await
}

fn seed_scenario(state: &Arc<ServerState>, replay: &mut ReplayState, scenario: &str) {
    match scenario {
        "idle_fleet" => {
            seed_ghosts(
                state,
                replay,
                &[
                    ReplayGhostSeed {
                        id: "replay-idle-01",
                        hostname: "ops-gateway-01",
                        os: "linux",
                        sleep_interval: 30,
                        jitter_percent: 5,
                    },
                    ReplayGhostSeed {
                        id: "replay-idle-02",
                        hostname: "db-mirror-01",
                        os: "linux",
                        sleep_interval: 30,
                        jitter_percent: 5,
                    },
                    ReplayGhostSeed {
                        id: "replay-idle-03",
                        hostname: "eng-workstation-01",
                        os: "linux",
                        sleep_interval: 30,
                        jitter_percent: 5,
                    },
                ],
            );
        }
        "task_flow" => {
            seed_ghosts(
                state,
                replay,
                &[
                    ReplayGhostSeed {
                        id: "replay-task-01",
                        hostname: "jumpbox-01",
                        os: "linux",
                        sleep_interval: 10,
                        jitter_percent: 3,
                    },
                    ReplayGhostSeed {
                        id: "replay-task-02",
                        hostname: "ops-laptop-01",
                        os: "linux",
                        sleep_interval: 10,
                        jitter_percent: 3,
                    },
                ],
            );

            push_history_task(
                state,
                "replay-task-01",
                Task {
                    id: "replay-task-history-01".to_string(),
                    command: "EXEC".to_string(),
                    args: "whoami".to_string(),
                    status: TaskStatus::Done,
                    result: Some("replay-operator".to_string()),
                },
            );

            push_history_task(
                state,
                "replay-task-02",
                Task {
                    id: "replay-task-history-02".to_string(),
                    command: "EXEC".to_string(),
                    args: "uname -a".to_string(),
                    status: TaskStatus::Done,
                    result: Some(
                        "Linux ops-laptop-01 6.8.0-replay #1 SMP PREEMPT_DYNAMIC x86_64 GNU/Linux"
                            .to_string(),
                    ),
                },
            );
        }
        "loot_burst" => {
            let now = Utc::now().timestamp();

            seed_ghosts(
                state,
                replay,
                &[
                    ReplayGhostSeed {
                        id: "replay-loot-01",
                        hostname: "fileserver-01",
                        os: "linux",
                        sleep_interval: 5,
                        jitter_percent: 1,
                    },
                    ReplayGhostSeed {
                        id: "replay-loot-02",
                        hostname: "finance-app-01",
                        os: "linux",
                        sleep_interval: 5,
                        jitter_percent: 1,
                    },
                    ReplayGhostSeed {
                        id: "replay-loot-03",
                        hostname: "backup-node-01",
                        os: "linux",
                        sleep_interval: 5,
                        jitter_percent: 1,
                    },
                ],
            );

            replay.scheduled_results.extend([
                ScheduledReplayResult {
                    ghost_id: "replay-loot-01".to_string(),
                    task_id: "replay-loot-auto-01".to_string(),
                    command: "EXFIL".to_string(),
                    args: "scheduled_wave_1".to_string(),
                    ready_at: now + 2,
                    status: TaskStatus::Done,
                    output: "replay mode: scheduled loot burst #1 complete :3".to_string(),
                    loot_filename: Some("replay_loot_burst_01.txt".to_string()),
                    loot_contents: Some(
                        "scheduled replay loot\nhost=fileserver-01\nwave=1\n".to_string(),
                    ),
                    remove_ghost: false,
                    from_pending: false,
                },
                ScheduledReplayResult {
                    ghost_id: "replay-loot-02".to_string(),
                    task_id: "replay-loot-auto-02".to_string(),
                    command: "EXFIL".to_string(),
                    args: "scheduled_wave_2".to_string(),
                    ready_at: now + 4,
                    status: TaskStatus::Done,
                    output: "replay mode: scheduled loot burst #2 complete :3".to_string(),
                    loot_filename: Some("replay_loot_burst_02.txt".to_string()),
                    loot_contents: Some(
                        "scheduled replay loot\nhost=finance-app-01\nwave=2\n".to_string(),
                    ),
                    remove_ghost: false,
                    from_pending: false,
                },
                ScheduledReplayResult {
                    ghost_id: "replay-loot-03".to_string(),
                    task_id: "replay-loot-auto-03".to_string(),
                    command: "EXFIL".to_string(),
                    args: "scheduled_wave_3".to_string(),
                    ready_at: now + 6,
                    status: TaskStatus::Done,
                    output: "replay mode: scheduled loot burst #3 complete :3".to_string(),
                    loot_filename: Some("replay_loot_burst_03.txt".to_string()),
                    loot_contents: Some(
                        "scheduled replay loot\nhost=backup-node-01\nwave=3\n".to_string(),
                    ),
                    remove_ghost: false,
                    from_pending: false,
                },
            ]);
        }
        _ => {}
    }
}

fn seed_ghosts(state: &Arc<ServerState>, replay: &mut ReplayState, seeds: &[ReplayGhostSeed]) {
    let now = Utc::now().timestamp();

    for seed in seeds {
        state.ghosts.insert(
            seed.id.to_string(),
            Ghost {
                id: seed.id.to_string(),
                hostname: seed.hostname.to_string(),
                os: seed.os.to_string(),
                sysinfo: None,
                sleep_interval: Some(seed.sleep_interval),
                jitter_percent: Some(seed.jitter_percent),
                update_pending: Some(false),
                last_seen: Some(now),
                is_replay: true,
            },
        );

        replay.replay_ghost_ids.insert(seed.id.to_string());
    }
}

fn push_history_task(state: &Arc<ServerState>, ghost_id: &str, task: Task) {
    state
        .task_history
        .entry(ghost_id.to_string())
        .or_insert_with(Vec::new)
        .push(task);
}

async fn write_replay_loot(filename: &str, contents: &str) -> Result<(), String> {
    tokio::fs::create_dir_all("loot")
        .await
        .map_err(|error| format!("failed to create replay loot dir :c [{}]", error))?;

    let path = format!("loot/{}", filename);

    tokio::fs::write(path, contents)
        .await
        .map_err(|error| format!("failed to write replay loot [{}]", error))
}

async fn remove_replay_loot_files() -> Result<(), String> {
    let Ok(mut entries) = tokio::fs::read_dir("loot").await else {
        return Ok(());
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };

        if name.starts_with(REPLAY_LOOT_PREFIX) {
            tokio::fs::remove_file(entry.path())
                .await
                .map_err(|error| {
                    format!("failed to remove replay loot :c ['{}': {}]", name, error)
                })?;
        }
    }

    Ok(())
}
