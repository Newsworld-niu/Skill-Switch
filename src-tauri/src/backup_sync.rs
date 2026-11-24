//! Background push of the backup clone.
//!
//! Mutations commit locally (fast, under the mutation lock) and then call
//! [`schedule_push`]. A single worker thread performs the network push, coalescing
//! bursts of mutations into as few pushes as possible, and broadcasts its state to
//! the frontend via the `backup-sync-state` event.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::Emitter;

use crate::domain::BackupSyncState;
use crate::{git, store};

pub const STATE_EVENT: &str = "backup-sync-state";
/// Emitted after the startup pull may have changed library contents.
pub const LIBRARY_CHANGED_EVENT: &str = "library-changed";

static PUSH_REQUESTED: AtomicBool = AtomicBool::new(false);
static WORKER_RUNNING: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<BackupSyncState> = Mutex::new(BackupSyncState {
    status: String::new(),
    last_error: None,
    last_synced_at: None,
});

pub fn current_state() -> BackupSyncState {
    let state = STATE.lock().unwrap_or_else(|p| p.into_inner()).clone();
    if state.status.is_empty() {
        BackupSyncState {
            status: "idle".into(),
            ..state
        }
    } else {
        state
    }
}

fn set_state(app: &tauri::AppHandle, next: BackupSyncState) {
    *STATE.lock().unwrap_or_else(|p| p.into_inner()) = next.clone();
    let _ = app.emit(STATE_EVENT, next);
}

/// Requests a push. Returns immediately; the push happens on a worker thread.
pub fn schedule_push(app: &tauri::AppHandle) {
    PUSH_REQUESTED.store(true, Ordering::SeqCst);
    if WORKER_RUNNING.swap(true, Ordering::SeqCst) {
        return; // the running worker will pick the request up
    }

    let app = app.clone();
    std::thread::spawn(move || loop {
        while PUSH_REQUESTED.swap(false, Ordering::SeqCst) {
            push_once(&app);
        }
        WORKER_RUNNING.store(false, Ordering::SeqCst);
        // A request may have arrived between the last check and clearing the flag.
        if !PUSH_REQUESTED.load(Ordering::SeqCst) || WORKER_RUNNING.swap(true, Ordering::SeqCst) {
            break;
        }
    });
}

fn push_once(app: &tauri::AppHandle) {
    let previous = current_state();
    set_state(
        app,
        BackupSyncState {
            status: "syncing".into(),
            ..previous.clone()
        },
    );

    let result = match store::persistent_clone_dir(app) {
        Ok(Some(dir)) if dir.exists() => git::push(&dir).map(|()| git::head(&dir)),
        Ok(_) => {
            set_state(
                app,
                BackupSyncState {
                    status: "idle".into(),
                    ..previous
                },
            );
            return;
        }
        Err(error) => Err(error),
    };

    let error = result.as_ref().err().cloned();
    let pushed_commit = result.ok().flatten();
    let _ = store::with_mutation_lock(|| {
        store::record_backup_push_result(app, pushed_commit, error.clone())
    });

    set_state(
        app,
        match error {
            None => BackupSyncState {
                status: "synced".into(),
                last_error: None,
                last_synced_at: Some(chrono::Utc::now().timestamp_millis()),
            },
            Some(error) => BackupSyncState {
                status: "failed".into(),
                last_error: Some(error),
                last_synced_at: previous.last_synced_at,
            },
        },
    );
}

/// Startup: pull remote changes, notify the UI to reload, then retry any
/// commits that were left unpushed by a previous session.
pub fn spawn_startup_sync(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let result = store::with_mutation_lock(|| store::backup_source_startup_sync(&app));
        match result {
            Ok(has_unpushed) => {
                let _ = app.emit(LIBRARY_CHANGED_EVENT, ());
                if has_unpushed {
                    schedule_push(&app);
                }
            }
            Err(error) => println!("[SkillSwitch] startup sync failed: {}", error),
        }
    });
}
