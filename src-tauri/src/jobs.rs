//! The job queue: plans jobs through their module, runs them in two lanes
//! (downloads in parallel, processing one at a time), streams progress to the
//! UI as `job-updated` events, writes a log per job and keeps history in SQLite.

use std::collections::{HashSet, VecDeque};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::{mpsc, watch};

use crate::db::Db;
use crate::errors::{self, Fix, FixContext};
use crate::guard;
use crate::inspect::{AUDIO_EXTENSIONS, VIDEO_EXTENSIONS};
use crate::modules::{self, JobInput, Lane, PlanContext, Step};
use crate::progress::{Feed, Parser, Source};
use crate::settings::{SettingsStore, SignIn};
use crate::tools;

/// Progress events are throttled to this, per job.
const EMIT_EVERY: Duration = Duration::from_millis(250);
/// How many jobs History keeps.
const HISTORY_LIMIT: u32 = 500;
/// Log lines kept in memory to explain a failure.
const TAIL_LINES: usize = 40;
/// Jobs waiting or running at once. More is refused, so dropping a huge folder
/// by mistake can't bury the app in work.
const MAX_ACTIVE_JOBS: usize = 2500; // above what a folder drop can add (inspect::expand)
/// A job won't start with less than this free where it writes…
const MIN_FREE_TO_START: u64 = 1024 * 1024 * 1024;
/// …and stops if a drive it writes to falls below this while it runs, before
/// Windows itself runs out of room.
const MIN_FREE_RUNNING: u64 = 500 * 1024 * 1024;
const SPACE_CHECK_EVERY: Duration = Duration::from_secs(10);
/// A tool that prints nothing for this long is stuck (a dead connection) and is
/// stopped. Every tool here reports progress far more often.
const STALL_LIMIT: Duration = Duration::from_secs(30 * 60);
/// Longer output lines are cut, so a tool spewing without newlines can't eat memory.
const MAX_LINE: usize = 64 * 1024;
/// Output lines buffered between a tool and the job; a faster tool waits.
const LINE_BUFFER: usize = 1024;
/// A job's log starts a new part past this, keeping one older part.
const LOG_PART: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
    /// The app closed while the job was queued or running.
    Interrupted,
}

impl Status {
    fn is_finished(self) -> bool {
        !matches!(self, Status::Queued | Status::Running)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StepView {
    pub label: String,
    /// Show command text, with sensitive arguments hidden.
    pub command: String,
}

/// A job as the UI and history see it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub module: String,
    pub title: String,
    pub lane: Lane,
    pub status: Status,
    pub inputs: Vec<String>,
    pub options: serde_json::Value,
    pub steps: Vec<StepView>,
    pub current_step: usize,
    /// Overall 0..=1, or None while unknown.
    pub progress: Option<f64>,
    pub eta_secs: Option<f64>,
    pub speed: Option<String>,
    /// Files the job made. Filled in once it's done.
    pub outputs: Vec<String>,
    /// Plain-language reason, when failed (or a warning, when done).
    pub error: Option<String>,
    /// A button that would help with `error`.
    #[serde(default)]
    pub fix: Option<Fix>,
    /// The job that started this one (a download's Premiere-safe follow-up).
    #[serde(default)]
    pub parent: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

/// What a planned job will run. Empty for jobs loaded from history.
#[derive(Clone, Default)]
struct Work {
    steps: Vec<Step>,
    /// Outputs known when the job was planned.
    outputs: Vec<PathBuf>,
    /// A file the tool writes its final output paths to, one per line (yt-dlp).
    output_list: Option<PathBuf>,
    /// Deleted when the job ends, however it ends.
    temp_files: Vec<PathBuf>,
    /// A failure that still produced files is done-with-a-warning (playlists).
    partial_ok: bool,
    /// Folders the job writes to, whose drives must keep some free space.
    space_dirs: Vec<PathBuf>,
}

struct Entry {
    job: Job,
    work: Work,
    cancel: Option<watch::Sender<bool>>,
    last_emit: Option<Instant>,
}

enum StepError {
    Cancelled,
    Failed(String),
}

pub struct Queue {
    app: AppHandle,
    db: Mutex<Db>,
    logs_dir: PathBuf,
    /// Per-job scratch files (cookie copies, output lists). Emptied at startup.
    temp_dir: PathBuf,
    reserved: Mutex<HashSet<PathBuf>>,
    entries: Mutex<Vec<Entry>>,
    /// When the last "all done" notice went out; only jobs finished after it are counted.
    notified_at: Mutex<i64>,
    /// Follow-up jobs (a download's Premiere-safe step) still being planned. Planning
    /// probes the file and can take a while, so the queue isn't "done" until they're in.
    planning: std::sync::atomic::AtomicUsize,
}

/// What the "queue finished" notice says.
fn notice_text(done: usize, failed: usize, only: Option<&str>) -> String {
    match (done, failed, only) {
        (1, 0, Some(title)) => format!("Done: {title}"),
        (0, 1, Some(title)) => format!("Couldn't finish: {title}"),
        (_, 0, _) => format!("All {done} jobs are done."),
        (0, _, _) => format!("{failed} jobs couldn't be finished."),
        _ => format!("{done} done, {failed} couldn't be finished."),
    }
}

/// Shows a desktop notification. Failing to show one is never an error worth reporting.
pub fn notify(app: &AppHandle, title: &str, body: &str) -> Result<(), String> {
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| e.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

impl Queue {
    pub fn new(app: AppHandle, db: Db, logs_dir: PathBuf, temp_dir: PathBuf) -> Arc<Self> {
        // Anything left here is from a previous run that crashed, possibly a cookies copy.
        if let Ok(stale) = std::fs::read_dir(&temp_dir) {
            for entry in stale.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        let _ = db.prune(HISTORY_LIMIT);
        let mut history = db.recent(HISTORY_LIMIT).unwrap_or_default();
        remove_stray_logs(&logs_dir, &history);
        for job in history.iter_mut().filter(|j| !j.status.is_finished()) {
            job.status = Status::Interrupted;
            job.error = Some("The app closed before this finished. Retry to run it again.".into());
            job.finished_at.get_or_insert_with(now_ms);
            let _ = db.save(job);
        }
        let entries = history
            .into_iter()
            .map(|job| Entry {
                job,
                work: Work::default(),
                cancel: None,
                last_emit: None,
            })
            .collect();
        Arc::new(Self {
            app,
            db: Mutex::new(db),
            logs_dir,
            temp_dir,
            reserved: Mutex::new(HashSet::new()),
            notified_at: Mutex::new(now_ms()),
            planning: std::sync::atomic::AtomicUsize::new(0),
            entries: Mutex::new(entries),
        })
    }

    pub fn temp_dir(&self) -> &Path {
        &self.temp_dir
    }

    pub fn list(&self) -> Vec<Job> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.job.clone())
            .collect()
    }

    fn emit(&self, job: &Job) {
        let _ = self.app.emit("job-updated", job);
    }

    fn persist(&self, job: &Job) {
        if let Err(e) = self.db.lock().unwrap().save(job) {
            eprintln!("couldn't save job {}: {e}", job.id);
        }
    }

    /// Applies `change` to a job and emits it, at most every [`EMIT_EVERY`] unless `force`.
    fn update(&self, id: &str, force: bool, change: impl FnOnce(&mut Job)) -> Option<Job> {
        let snapshot = {
            let mut entries = self.entries.lock().unwrap();
            let entry = entries.iter_mut().find(|e| e.job.id == id)?;
            change(&mut entry.job);
            let now = Instant::now();
            let due = force
                || entry
                    .last_emit
                    .is_none_or(|t| now.duration_since(t) >= EMIT_EVERY);
            if !due {
                return None;
            }
            entry.last_emit = Some(now);
            entry.job.clone()
        };
        self.emit(&snapshot);
        Some(snapshot)
    }

    fn release(&self, outputs: &[PathBuf]) {
        let mut reserved = self.reserved.lock().unwrap();
        for o in outputs {
            reserved.remove(o);
        }
    }

    /// Plans one job per input and queues them. Inputs that can't be planned become
    /// failed jobs with the reason, so the user sees why instead of nothing happening.
    pub async fn submit(
        self: &Arc<Self>,
        module_id: &str,
        inputs: Vec<String>,
        options: serde_json::Value,
    ) -> Result<Vec<String>, String> {
        self.submit_as(module_id, inputs, options, None).await
    }

    async fn submit_as(
        self: &Arc<Self>,
        module_id: &str,
        inputs: Vec<String>,
        options: serde_json::Value,
        parent: Option<String>,
    ) -> Result<Vec<String>, String> {
        let module =
            modules::find(module_id).ok_or_else(|| format!("Unknown tool: {module_id}"))?;
        let active = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| !e.job.status.is_finished())
            .count();
        let adding = if module.combines_inputs() {
            1
        } else {
            inputs.len()
        };
        if active + adding > MAX_ACTIVE_JOBS {
            return Err(format!(
                "That would put more than {MAX_ACTIVE_JOBS} jobs in the queue ({active} already waiting or running). Let some finish, or add fewer at once."
            ));
        }
        let mut ids = Vec::new();

        // One job per input, or one job for all of them (Trim & merge).
        let groups: Vec<Vec<String>> = if module.combines_inputs() {
            vec![inputs]
        } else {
            inputs.into_iter().map(|i| vec![i]).collect()
        };

        for group in groups {
            let this = Arc::clone(self);
            let opts = options.clone();
            let planned = tauri::async_runtime::spawn_blocking(move || {
                let mut inputs = Vec::new();
                for raw in &group {
                    let name = raw.rsplit(['\\', '/']).next().unwrap_or(raw).to_string();
                    inputs.push(JobInput::parse(raw).map_err(|e| (name, e))?);
                }
                let display = match inputs.as_slice() {
                    [one] => one.short_name(),
                    many => format!("{} files", many.len()),
                };
                let ctx = PlanContext {
                    app: &this.app,
                    reserved: &this.reserved,
                    temp_dir: &this.temp_dir,
                };
                let plan = if module.combines_inputs() {
                    module.plan_combined(&ctx, &inputs, &opts)
                } else {
                    module.plan(&ctx, &inputs[0], &opts)
                };
                match plan {
                    Ok(plan) => Ok((inputs, plan)),
                    Err(e) => Err((display, e)),
                }
            })
            .await
            .map_err(|e| e.to_string())?;

            // Nothing to do (e.g. already Premiere-safe after a download): no job.
            if let Ok((_, plan)) = &planned
                && plan.skip.is_some()
            {
                continue;
            }

            let now = now_ms();
            let mut job = Job {
                id: uuid::Uuid::new_v4().to_string(),
                module: module.id().into(),
                title: String::new(),
                lane: module.lane(),
                status: Status::Queued,
                inputs: Vec::new(),
                options: options.clone(),
                steps: Vec::new(),
                current_step: 0,
                progress: None,
                eta_secs: None,
                speed: None,
                outputs: Vec::new(),
                error: None,
                fix: None,
                parent: parent.clone(),
                created_at: now,
                started_at: None,
                finished_at: None,
            };
            let work = match planned {
                Ok((inputs, plan)) => {
                    job.title = plan.title;
                    job.inputs = inputs.iter().map(JobInput::as_string).collect();
                    job.steps = plan
                        .steps
                        .iter()
                        .map(|s| StepView {
                            label: s.label.clone(),
                            command: s.command_line(),
                        })
                        .collect();
                    let mut space_dirs = plan.writes_to;
                    space_dirs.extend(
                        plan.outputs
                            .iter()
                            .filter_map(|o| o.parent().map(Path::to_path_buf)),
                    );
                    space_dirs.push(self.temp_dir.clone());
                    space_dirs.sort();
                    space_dirs.dedup();
                    Work {
                        steps: plan.steps,
                        outputs: plan.outputs,
                        output_list: plan.output_list,
                        temp_files: plan.temp_files,
                        partial_ok: plan.partial_ok,
                        space_dirs,
                    }
                }
                Err((name, reason)) => {
                    job.title = format!("{}: {name}", module.name());
                    job.inputs = vec![name];
                    job.status = Status::Failed;
                    job.fix = self.fix_for(&job.module, &job.options, &reason);
                    job.error = Some(reason);
                    job.finished_at = Some(now);
                    Work::default()
                }
            };

            self.persist(&job);
            self.emit(&job);
            ids.push(job.id.clone());
            self.entries.lock().unwrap().push(Entry {
                job,
                work,
                cancel: None,
                last_emit: None,
            });
        }

        self.schedule();
        Ok(ids)
    }

    /// Starts queued jobs while their lane has room.
    fn schedule(self: &Arc<Self>) {
        let download_slots = self.app.state::<SettingsStore>().get().download_concurrency as usize;
        let mut starting = Vec::new();
        {
            let mut entries = self.entries.lock().unwrap();
            let running = |lane: Lane, entries: &[Entry]| {
                entries
                    .iter()
                    .filter(|e| e.job.status == Status::Running && e.job.lane == lane)
                    .count()
            };
            let mut downloads = running(Lane::Download, &entries);
            let mut processing = running(Lane::Process, &entries);

            for entry in entries
                .iter_mut()
                .filter(|e| e.job.status == Status::Queued)
            {
                let slot = match entry.job.lane {
                    Lane::Download => &mut downloads,
                    Lane::Process => &mut processing,
                };
                let limit = if entry.job.lane == Lane::Download {
                    download_slots
                } else {
                    1
                };
                if *slot >= limit {
                    continue;
                }
                *slot += 1;
                let (tx, rx) = watch::channel(false);
                entry.cancel = Some(tx);
                entry.job.status = Status::Running;
                entry.job.started_at = Some(now_ms());
                starting.push((entry.job.clone(), entry.work.clone(), rx));
            }
        }

        let busy = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.job.status == Status::Running);
        guard::keep_awake(busy);

        for (job, work, cancel) in starting {
            self.persist(&job);
            self.emit(&job);
            let this = Arc::clone(self);
            tauri::async_runtime::spawn(async move { this.run(job.id, work, cancel).await });
        }
    }

    async fn run(self: Arc<Self>, id: String, work: Work, mut cancel: watch::Receiver<bool>) {
        let mut log = LogFile::create(&self.logs_dir.join(format!("{id}.log")));
        let outcome = self.run_steps(&id, &work, &mut log, &mut cancel).await;
        for temp in &work.temp_files {
            let _ = std::fs::remove_file(temp);
        }

        let (status, error, outputs) = match outcome {
            // A done job's `error` is a warning, e.g. some playlist videos failed.
            Ok((outputs, warning)) => (Status::Done, warning, outputs),
            Err(StepError::Cancelled) => (Status::Cancelled, None, Vec::new()),
            Err(StepError::Failed(reason)) => (Status::Failed, Some(reason), Vec::new()),
        };
        if let Some(reason) = &error {
            let label = if status == Status::Done {
                "Warning"
            } else {
                "Failed"
            };
            log.line(&format!("{label}: {reason}"));
        }
        self.release(&work.outputs);

        let fix = error.as_deref().and_then(|reason| {
            let (module, options) = self
                .entries
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.job.id == id)
                .map(|e| (e.job.module.clone(), e.job.options.clone()))?;
            self.fix_for(&module, &options, reason)
        });
        let finished = self.update(&id, true, |job| {
            job.status = status;
            job.error = error;
            job.fix = fix;
            job.finished_at = Some(now_ms());
            job.eta_secs = None;
            job.speed = None;
            if status == Status::Done {
                job.progress = Some(1.0);
                job.outputs = outputs.iter().map(|o| o.display().to_string()).collect();
            }
        });
        if let Some(job) = &finished {
            self.persist(job);
        }
        if let Some(entry) = self
            .entries
            .lock()
            .unwrap()
            .iter_mut()
            .find(|e| e.job.id == id)
        {
            entry.cancel = None;
        }
        if let Some(job) = finished.filter(|j| j.status == Status::Done) {
            self.follow_up(&job);
        }
        self.schedule();
        self.notify_when_idle();
    }

    /// A desktop notice once nothing is left to run, if the window isn't in front.
    /// Waits a moment first: a finished download starts its follow-up job a beat later.
    fn notify_when_idle(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            if this.planning.load(std::sync::atomic::Ordering::SeqCst) > 0 {
                return; // the follow-up's own finish will announce everything
            }
            let settings = this.app.state::<SettingsStore>().get();
            let in_front = this
                .app
                .get_webview_window("main")
                .and_then(|w| w.is_focused().ok())
                .unwrap_or(false);
            let (done, failed, only) = {
                let entries = this.entries.lock().unwrap();
                let mut notified = this.notified_at.lock().unwrap();
                if entries.iter().any(|e| !e.job.status.is_finished()) {
                    return;
                }
                let fresh: Vec<&Job> = entries
                    .iter()
                    .map(|e| &e.job)
                    .filter(|j| j.finished_at.unwrap_or(0) > *notified)
                    .collect();
                if fresh.is_empty() {
                    return;
                }
                *notified = now_ms();
                let done = fresh.iter().filter(|j| j.status == Status::Done).count();
                let failed = fresh
                    .iter()
                    .filter(|j| matches!(j.status, Status::Failed | Status::Interrupted))
                    .count();
                let only = (fresh.len() == 1).then(|| fresh[0].title.clone());
                (done, failed, only)
            };
            if in_front || !settings.notify_when_done {
                return;
            }
            let body = notice_text(done, failed, only.as_deref());
            let _ = notify(&this.app, "Crude's Toolkit", &body);
        });
    }

    /// Runs the job's `then` module on its outputs, e.g. a post-download step.
    /// Options: `{"then": "<module id>", "thenOptions": {...}}`.
    fn follow_up(self: &Arc<Self>, job: &Job) {
        let Some(next) = job.options.get("then").and_then(|v| v.as_str()) else {
            return;
        };
        // Only the videos: a download also saves chat and the like.
        let inputs: Vec<String> = job
            .outputs
            .iter()
            .filter(|o| is_video(Path::new(o)))
            .cloned()
            .collect();
        if inputs.is_empty() {
            return;
        }
        let next = next.to_string();
        let options = job
            .options
            .get("thenOptions")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let this = Arc::clone(self);
        let parent = job.id.clone();
        self.planning
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        tauri::async_runtime::spawn(async move {
            let added = this.submit_as(&next, inputs, options, Some(parent)).await;
            this.planning
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            // No follow-up job means nothing else will announce that the queue is done.
            if added.is_err() {
                this.notify_when_idle();
            }
        });
    }

    /// The button to offer next to a job's error, if one would help.
    fn fix_for(&self, module: &str, options: &serde_json::Value, message: &str) -> Option<Fix> {
        let settings = self.app.state::<SettingsStore>().get();
        let ctx = FixContext {
            module,
            signed_in: options
                .get("signIn")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            sign_in_ready: settings.sign_in != SignIn::None,
        };
        errors::fix_for(message, &ctx)
    }

    /// Runs every step, then works out what the job produced, plus a warning for
    /// a partial success. On failure or cancel, removes files the job started
    /// writing; never anything that existed before.
    async fn run_steps(
        &self,
        id: &str,
        work: &Work,
        log: &mut LogFile,
        cancel: &mut watch::Receiver<bool>,
    ) -> Result<(Vec<PathBuf>, Option<String>), StepError> {
        if let Some(message) = low_space_checked(&work.space_dirs, MIN_FREE_TO_START).await {
            return Err(StepError::Failed(message));
        }

        // Outputs were free when the job was planned. If one appeared since, stop
        // rather than risk touching a file this job didn't make.
        for out in &work.outputs {
            if out.exists() {
                return Err(StepError::Failed(format!(
                    "{} already exists. Retry to pick a new name.",
                    out.display()
                )));
            }
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir).map_err(|e| {
                    StepError::Failed(format!("Couldn't create {}: {e}", dir.display()))
                })?;
            }
        }

        // What the tools said they were writing, and what they found already there.
        let mut touched = Touched::default();
        let ran = self.run_all(id, work, log, cancel, &mut touched).await;
        let Touched {
            created, reused, ..
        } = touched;
        let result = match ran {
            Ok(()) => collect_outputs(work).map(|outputs| {
                let warning = if work.partial_ok && outputs.is_empty() {
                    Some("Nothing new to download: everything that matched was already downloaded, or nothing matched the dates.".to_string())
                } else {
                    reuse_warning(&outputs, &reused)
                };
                (outputs, warning)
            }),
            // A playlist that failed or was stopped partway: keep the videos that
            // finished. They're in the download archive, so deleting them would
            // make the next run skip them.
            Err(e) if work.partial_ok => match collect_outputs(work) {
                Ok(outputs) if !outputs.is_empty() => {
                    let warning = match e {
                        StepError::Failed(reason) => format!(
                            "Some videos couldn't be downloaded. {reason} The rest are saved; the log lists which."
                        ),
                        StepError::Cancelled => "Stopped early. The videos that finished are saved; run it again to get the rest.".into(),
                    };
                    Ok((outputs, Some(warning)))
                }
                _ => Err(e),
            },
            Err(e) => Err(e),
        };

        // Safe: none of these existed when the job started, so they're ours.
        let partials = |path: &PathBuf| {
            [".part", ".ytdl"].map(|suffix| {
                let mut p = path.clone().into_os_string();
                p.push(suffix);
                PathBuf::from(p)
            })
        };
        match &result {
            Err(_) => {
                for out in work.outputs.iter().chain(&created) {
                    remove_eventually(out).await;
                    for p in partials(out) {
                        remove_eventually(&p).await;
                    }
                }
            }
            // After a partial success only unfinished videos' pieces go; a saved
            // video keeps the files beside it (chat, subtitles).
            Ok((outputs, Some(_))) => {
                for path in &created {
                    if !belongs_to_output(path, outputs) {
                        remove_eventually(path).await;
                    }
                    for p in partials(path) {
                        remove_eventually(&p).await;
                    }
                }
            }
            Ok(_) => {}
        }
        result
    }

    async fn run_all(
        &self,
        id: &str,
        work: &Work,
        log: &mut LogFile,
        cancel: &mut watch::Receiver<bool>,
        touched: &mut Touched,
    ) -> Result<(), StepError> {
        let steps = &work.steps;
        for (index, step) in steps.iter().enumerate() {
            self.update(id, true, |job| job.current_step = index);
            log.line(&format!("$ {}", step.command_line()));
            let at = StepAt {
                index,
                total: steps.len(),
                space_dirs: &work.space_dirs,
            };
            self.run_step(id, at, step, log, cancel, touched).await?;
        }
        Ok(())
    }

    async fn run_step(
        &self,
        id: &str,
        at: StepAt<'_>,
        step: &Step,
        log: &mut LogFile,
        cancel: &mut watch::Receiver<bool>,
        touched: &mut Touched,
    ) -> Result<(), StepError> {
        let path = step.tool.path(&self.app).map_err(StepError::Failed)?;
        let mut child = tools::spawn(tools::async_command(&path).args(&step.args))
            .map_err(|e| StepError::Failed(format!("Couldn't start {}: {e}", step.tool.name())))?;

        let (tx, mut lines) = mpsc::channel(LINE_BUFFER);
        spawn_reader(child.stdout.take(), Source::Stdout, tx.clone());
        spawn_reader(child.stderr.take(), Source::Stderr, tx);

        let mut ctx = LineCtx {
            parser: Parser::new(&step.progress),
            tail: VecDeque::with_capacity(TAIL_LINES),
            hidden: step
                .redact
                .iter()
                .filter_map(|&i| step.args.get(i).cloned())
                .collect(),
            index: at.index,
            total: at.total,
            touched,
        };

        let mut space_check = tokio::time::interval(SPACE_CHECK_EVERY);
        space_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut last_output = tokio::time::Instant::now();
        let status = loop {
            tokio::select! {
                Some((source, line)) = lines.recv() => {
                    last_output = tokio::time::Instant::now();
                    self.handle_line(id, &mut ctx, log, source, line);
                }
                status = child.wait() => break status,
                changed = cancel.changed() => {
                    if changed.is_err() || *cancel.borrow() {
                        tools::kill_tree(&mut child).await;
                        log.line("Cancelled.");
                        return Err(StepError::Cancelled);
                    }
                }
                _ = space_check.tick() => {
                    if let Some(message) = low_space_checked(at.space_dirs, MIN_FREE_RUNNING).await {
                        tools::kill_tree(&mut child).await;
                        return Err(StepError::Failed(format!("Stopped to protect your computer. {message}")));
                    }
                }
                _ = tokio::time::sleep_until(last_output + STALL_LIMIT) => {
                    tools::kill_tree(&mut child).await;
                    return Err(StepError::Failed(format!(
                        "{} showed no sign of life for {} minutes, so it was stopped. Its connection may have dropped; retry.",
                        step.tool.name(),
                        STALL_LIMIT.as_secs() / 60
                    )));
                }
            }
        };

        // Collect what the tool printed just before exiting.
        let drain = async {
            while let Some((source, line)) = lines.recv().await {
                self.handle_line(id, &mut ctx, log, source, line);
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(2), drain).await;

        match status {
            Ok(s) if s.success() => Ok(()),
            // yt-dlp stopped on purpose: a playlist reached videos older than the dates.
            Ok(s) if step.tool == tools::Tool::YtDlp && s.code() == Some(101) => Ok(()),
            Ok(s) => {
                let tail: Vec<String> = ctx.tail.into_iter().collect();
                let signed_in = step
                    .args
                    .iter()
                    .any(|a| a == "--cookies" || a == "--cookies-from-browser");
                Err(StepError::Failed(errors::explain(
                    step.tool,
                    s.code(),
                    &tail,
                    signed_in,
                )))
            }
            Err(e) => Err(StepError::Failed(format!(
                "{} stopped unexpectedly: {e}",
                step.tool.name()
            ))),
        }
    }

    fn handle_line(
        &self,
        id: &str,
        ctx: &mut LineCtx<'_>,
        log: &mut LogFile,
        source: Source,
        mut line: String,
    ) {
        for secret in &ctx.hidden {
            line = line.replace(secret.as_str(), "<hidden>");
        }
        match ctx.parser.feed(source, &line) {
            Feed::Update(p) => {
                let (index, total) = (ctx.index as f64, ctx.total as f64);
                self.update(id, false, |job| {
                    job.progress = p.fraction.map(|f| (index + f) / total);
                    job.eta_secs = p.eta_secs;
                    job.speed = p.speed;
                });
            }
            Feed::Consumed => {}
            Feed::Creating(path) => {
                if ctx.touched.is_ours(&path) && !ctx.touched.created.contains(&path) {
                    ctx.touched.created.push(path);
                }
                ctx.log(log, line);
            }
            Feed::Reused(path) => {
                if !ctx.touched.reused.contains(&path) {
                    ctx.touched.reused.push(path);
                }
                ctx.log(log, line);
            }
            Feed::Log => ctx.log(log, line),
        }
    }

    pub fn cancel(self: &Arc<Self>, id: &str) {
        let cancelled = {
            let mut entries = self.entries.lock().unwrap();
            let Some(entry) = entries.iter_mut().find(|e| e.job.id == id) else {
                return;
            };
            match entry.job.status {
                Status::Queued => {
                    entry.job.status = Status::Cancelled;
                    entry.job.finished_at = Some(now_ms());
                    Some((entry.job.clone(), entry.work.outputs.clone()))
                }
                Status::Running => {
                    if let Some(tx) = &entry.cancel {
                        let _ = tx.send(true);
                    }
                    None
                }
                _ => None,
            }
        };
        if let Some((job, outputs)) = cancelled {
            self.release(&outputs);
            self.persist(&job);
            self.emit(&job);
        }
    }

    /// Runs a finished job again as a new job, re-planned so it gets a fresh output name.
    pub async fn retry(self: &Arc<Self>, id: &str) -> Result<Vec<String>, String> {
        let job = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.job.id == id)
            .map(|e| e.job.clone())
            .ok_or("That job is no longer in the list.")?;
        self.submit(&job.module, job.inputs, job.options).await
    }

    /// Removes finished jobs from history and deletes their logs. Files they made stay.
    pub fn clear_finished(&self) -> Result<(), String> {
        let removed: Vec<String> = {
            let mut entries = self.entries.lock().unwrap();
            let ids = entries
                .iter()
                .filter(|e| e.job.status.is_finished())
                .map(|e| e.job.id.clone())
                .collect();
            entries.retain(|e| !e.job.status.is_finished());
            ids
        };
        for id in &removed {
            remove_log(&self.logs_dir, id);
        }
        let _ = self.app.emit("jobs-removed", &removed);
        self.db.lock().unwrap().delete(&removed)
    }

    /// The job's log, trimmed to the last 256 KB.
    pub fn log(&self, id: &str) -> String {
        const MAX: usize = 256 * 1024;
        let Ok(bytes) = std::fs::read(self.logs_dir.join(format!("{id}.log"))) else {
            return String::new();
        };
        let start = bytes.len().saturating_sub(MAX);
        String::from_utf8_lossy(&bytes[start..]).into_owned()
    }

    /// The first file a finished job made, if it still exists.
    pub fn output(&self, id: &str) -> Result<PathBuf, String> {
        let entries = self.entries.lock().unwrap();
        let job = &entries
            .iter()
            .find(|e| e.job.id == id)
            .ok_or("That job is no longer in the list.")?
            .job;
        let path = job
            .outputs
            .first()
            .map(PathBuf::from)
            .ok_or("This job didn't make a file.")?;
        if !path.exists() {
            return Err(format!("{} has been moved or deleted.", path.display()));
        }
        Ok(path)
    }

    /// Stops running tools when the app quits, so no ffmpeg is left running in the background.
    pub fn shutdown(&self) {
        let mut any = false;
        for entry in self.entries.lock().unwrap().iter() {
            if let Some(tx) = &entry.cancel {
                let _ = tx.send(true);
                any = true;
            }
        }
        if any {
            std::thread::sleep(Duration::from_millis(400));
        }
    }
}

/// Files the tools reported while a job ran.
struct Touched {
    /// Announced as being written and not there before: ours to delete on failure.
    created: Vec<PathBuf>,
    /// Already in the folder, so the tool used them instead of downloading again.
    reused: Vec<PathBuf>,
    /// When the job began (with a little slack for file clocks). A file the tool
    /// says it's writing counts as ours only if it didn't exist, or was written
    /// since then: yt-dlp prints "Writing …" just after the file appears.
    started: SystemTime,
}

impl Default for Touched {
    fn default() -> Self {
        Self {
            created: Vec::new(),
            reused: Vec::new(),
            started: SystemTime::now() - Duration::from_secs(2),
        }
    }
}

impl Touched {
    fn is_ours(&self, path: &Path) -> bool {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .is_ok_and(|modified| modified >= self.started)
            || !path.exists()
    }
}

/// Tells the person when a "download" saved nothing new because the same file
/// was already there (yt-dlp never overwrites), instead of calling it a success.
fn reuse_warning(outputs: &[PathBuf], reused: &[PathBuf]) -> Option<String> {
    let reused_outputs: Vec<&PathBuf> = outputs.iter().filter(|o| reused.contains(o)).collect();
    match reused_outputs.as_slice() {
        [] => None,
        [one] => Some(format!(
            "{} was already in the folder, so it was used as it is and nothing new was downloaded. Rename or delete it to download again.",
            one.file_name().unwrap_or_default().to_string_lossy()
        )),
        many => Some(format!(
            "{} of these files were already in the folder and were used as they are. Rename or delete them to download again.",
            many.len()
        )),
    }
}

/// Where a step sits in its job, and where the job writes.
struct StepAt<'a> {
    index: usize,
    total: usize,
    space_dirs: &'a [PathBuf],
}

/// [`low_space`] off the async runtime, and never waiting on a drive that
/// stopped answering for more than a few seconds (then there's no verdict).
async fn low_space_checked(dirs: &[PathBuf], min: u64) -> Option<String> {
    let dirs = dirs.to_vec();
    let check = tauri::async_runtime::spawn_blocking(move || low_space(&dirs, min));
    tokio::time::timeout(Duration::from_secs(5), check)
        .await
        .ok()?
        .ok()?
}

/// "Only 400 MB free on D:\. …" for the first folder below `min`, if any.
fn low_space(dirs: &[PathBuf], min: u64) -> Option<String> {
    let (dir, free) = dirs
        .iter()
        .find_map(|d| guard::free_space(d).filter(|f| *f < min).map(|f| (d, f)))?;
    let drive = dir.ancestors().last().map_or_else(
        || dir.display().to_string(),
        |root| root.display().to_string(),
    );
    Some(format!(
        "Only {} is free on {drive}, where this job saves files. Free up some space, then retry.",
        size_text(free)
    ))
}

fn size_text(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let mb = bytes as f64 / MB;
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

/// A job's log and its older part, if any.
fn remove_log(logs_dir: &Path, id: &str) {
    let _ = std::fs::remove_file(logs_dir.join(format!("{id}.log")));
    let _ = std::fs::remove_file(logs_dir.join(format!("{id}.1.log")));
}

/// Deletes logs of jobs no longer in history (pruned, or cleared while the app was closing).
fn remove_stray_logs(logs_dir: &Path, history: &[Job]) {
    let keep: HashSet<&str> = history.iter().map(|j| j.id.as_str()).collect();
    let Ok(dir) = std::fs::read_dir(logs_dir) else {
        return;
    };
    for entry in dir.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(id) = name
            .strip_suffix(".1.log")
            .or_else(|| name.strip_suffix(".log"))
        else {
            continue;
        };
        // Only job logs (uuids); leave anything else, like crash.log, alone.
        if uuid::Uuid::parse_str(id).is_ok() && !keep.contains(id) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

struct LineCtx<'a> {
    parser: Parser,
    tail: VecDeque<String>,
    /// Redacted argument values, scrubbed from anything the tool prints.
    hidden: Vec<String>,
    index: usize,
    total: usize,
    touched: &'a mut Touched,
}

impl LineCtx<'_> {
    fn log(&mut self, log: &mut LogFile, line: String) {
        log.line(&line);
        if self.tail.len() == TAIL_LINES {
            self.tail.pop_front();
        }
        self.tail.push_back(line);
    }
}

fn is_video(path: &Path) -> bool {
    has_extension(path, VIDEO_EXTENSIONS)
}

fn is_audio(path: &Path) -> bool {
    has_extension(path, AUDIO_EXTENSIONS)
}

fn has_extension(path: &Path, list: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| list.contains(&e.to_ascii_lowercase().as_str()))
}

/// `path` is a saved output, or a file named after one ("video.live_chat.json").
fn belongs_to_output(path: &Path, outputs: &[PathBuf]) -> bool {
    let path = path.to_string_lossy();
    outputs.iter().any(|out| {
        let stem = out.with_extension("");
        *path == *out.to_string_lossy() || path.starts_with(&format!("{}.", stem.display()))
    })
}

/// Outputs known at plan time must exist; a tool's output list adds the files it
/// reported (yt-dlp names its files only once it has the video's details).
fn collect_outputs(work: &Work) -> Result<Vec<PathBuf>, StepError> {
    if let Some(missing) = work.outputs.iter().find(|o| !o.is_file()) {
        return Err(StepError::Failed(format!(
            "The tool finished but {} wasn't created.",
            missing.display()
        )));
    }
    let mut outputs = work.outputs.clone();
    if let Some(list) = &work.output_list {
        let listed = std::fs::read_to_string(list).unwrap_or_default();
        for line in listed.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let path = PathBuf::from(line);
            if path.is_file() && !outputs.contains(&path) {
                outputs.push(path);
            }
        }
        // The video comes first, then what was saved beside it (chat, subtitles),
        // so "Open" and the result screens lead with the video.
        outputs.sort_by_key(|p| !(is_video(p) || is_audio(p)));
        // A playlist where everything was already downloaded saves nothing; that's fine.
        if outputs.is_empty() && !work.partial_ok {
            return Err(StepError::Failed(
                "The download finished but no file was saved. The log has the details.".into(),
            ));
        }
    }
    Ok(outputs)
}

/// Deletes a file, retrying for a few seconds while another process still has it
/// open (Windows refuses to delete open files). Missing files are fine.
async fn remove_eventually(path: &Path) {
    for _ in 0..10 {
        match std::fs::remove_file(path) {
            Ok(()) => return,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) => tokio::time::sleep(Duration::from_millis(300)).await,
        }
    }
}

fn spawn_reader<R>(pipe: Option<R>, source: Source, tx: mpsc::Sender<(Source, String)>)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let Some(mut pipe) = pipe else { return };
    tauri::async_runtime::spawn(async move {
        let mut chunk = [0u8; 8192];
        let mut line = Vec::new();
        // Tools redraw progress with \r; split on it too so each update is its own
        // line. Output isn't always UTF-8 on Windows, so decode lossily.
        let send = |line: &mut Vec<u8>| {
            let text = String::from_utf8_lossy(line).trim_end().to_string();
            line.clear();
            text
        };
        loop {
            let n = match pipe.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            for &byte in &chunk[..n] {
                if byte == b'\n' || byte == b'\r' {
                    let text = send(&mut line);
                    if !text.is_empty() && tx.send((source, text)).await.is_err() {
                        return;
                    }
                } else if line.len() < MAX_LINE {
                    line.push(byte);
                }
            }
        }
        let text = send(&mut line);
        if !text.is_empty() {
            let _ = tx.send((source, text)).await;
        }
    });
}

/// A job's log file. Logging is best-effort and never fails a job. Past
/// [`LOG_PART`] the log moves to `<id>.1.log` (replacing an older one) and a
/// fresh part starts, so a long job keeps its latest output without filling the drive.
struct LogFile {
    file: Option<std::fs::File>,
    path: PathBuf,
    written: u64,
    part: u64,
}

impl LogFile {
    fn create(path: &Path) -> Self {
        Self {
            file: std::fs::File::create(path).ok(),
            path: path.to_path_buf(),
            written: 0,
            part: LOG_PART,
        }
    }

    fn line(&mut self, text: &str) {
        if self.written > self.part {
            self.file = None;
            let older = self.path.with_extension("1.log");
            let _ = std::fs::rename(&self.path, &older);
            self.file = std::fs::File::create(&self.path).ok();
            self.written = 0;
            let note = format!(
                "(Earlier output is in {})",
                older.file_name().unwrap_or_default().to_string_lossy()
            );
            self.write(&note);
        }
        self.write(text);
    }

    fn write(&mut self, text: &str) {
        if let Some(f) = &mut self.file
            && writeln!(f, "{text}").is_ok()
        {
            self.written += text.len() as u64 + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_beside_outputs() {
        let outputs = [PathBuf::from(r"D:\Dl\List\a b.mp4")];
        assert!(belongs_to_output(
            Path::new(r"D:\Dl\List\a b.mp4"),
            &outputs
        ));
        assert!(belongs_to_output(
            Path::new(r"D:\Dl\List\a b.live_chat.json"),
            &outputs
        ));
        assert!(!belongs_to_output(
            Path::new(r"D:\Dl\List\a bc.f137.mp4"),
            &outputs
        ));
        assert!(!belongs_to_output(
            Path::new(r"D:\Dl\List\other.mp4"),
            &outputs
        ));
    }

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("crude-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn log_keeps_two_parts() {
        let dir = scratch();
        let id = uuid::Uuid::new_v4().to_string();
        let mut log = LogFile::create(&dir.join(format!("{id}.log")));
        log.part = 100;
        for i in 0..50 {
            log.line(&format!("line {i:02} of output"));
        }
        let current = std::fs::read_to_string(dir.join(format!("{id}.log"))).unwrap();
        let older = std::fs::read_to_string(dir.join(format!("{id}.1.log"))).unwrap();
        assert!(current.starts_with("(Earlier output is in"));
        assert!(current.contains("line 49"));
        assert!(older.len() < 200);
        remove_log(&dir, &id);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stray_logs_go() {
        let dir = scratch();
        let kept = uuid::Uuid::new_v4().to_string();
        let gone = uuid::Uuid::new_v4().to_string();
        for name in [
            format!("{kept}.log"),
            format!("{gone}.log"),
            format!("{gone}.1.log"),
            "crash.log".into(),
        ] {
            std::fs::write(dir.join(name), "x").unwrap();
        }
        let job: Job = serde_json::from_value(serde_json::json!({
            "id": kept, "module": "remux", "title": "t", "lane": "process", "status": "done",
            "inputs": [], "options": null, "steps": [], "currentStep": 0, "progress": null,
            "etaSecs": null, "speed": null, "outputs": [], "error": null, "createdAt": 0,
            "startedAt": null, "finishedAt": null
        }))
        .unwrap();
        remove_stray_logs(&dir, &[job]);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        let mut want = vec!["crash.log".to_string(), format!("{kept}.log")];
        want.sort();
        assert_eq!(left, want);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sizes_read_plainly() {
        assert_eq!(size_text(400 * 1024 * 1024), "400 MB");
        assert_eq!(size_text(3 * 1024 * 1024 * 1024 / 2), "1.5 GB");
    }

    #[tokio::test]
    async fn reader_splits_and_caps_lines() {
        let mut out = b"first\r\nsecond\rthird\n".to_vec();
        out.extend(std::iter::repeat_n(b'x', MAX_LINE * 3));
        out.extend(b"\nlast");
        let (tx, mut rx) = mpsc::channel(4);
        spawn_reader(Some(std::io::Cursor::new(out)), Source::Stdout, tx);
        let mut lines = Vec::new();
        while let Some((_, line)) = rx.recv().await {
            lines.push(line);
        }
        assert_eq!(&lines[..3], ["first", "second", "third"]);
        assert_eq!(lines[3].len(), MAX_LINE);
        assert_eq!(lines[4], "last");
    }

    #[test]
    fn reused_files_are_called_out() {
        let dir = std::env::temp_dir();
        let a = dir.join("a.mp4");
        let b = dir.join("b.mp4");
        assert_eq!(reuse_warning(std::slice::from_ref(&a), &[]), None);
        assert_eq!(
            reuse_warning(std::slice::from_ref(&a), std::slice::from_ref(&b)),
            None
        );
        let one = reuse_warning(std::slice::from_ref(&a), std::slice::from_ref(&a)).unwrap();
        assert!(one.starts_with("a.mp4 was already in the folder"), "{one}");
        let two = reuse_warning(&[a.clone(), b.clone()], &[a, b]).unwrap();
        assert!(two.starts_with("2 of these files"), "{two}");
    }

    #[test]
    fn the_video_leads_its_side_files() {
        let dir = scratch();
        let chat = dir.join("a.chat.json");
        let video = dir.join("a.mp4");
        let list = dir.join("list.txt");
        std::fs::write(&chat, "{}").unwrap();
        std::fs::write(&video, "x").unwrap();
        std::fs::write(&list, format!("{}\n{}\n", chat.display(), video.display())).unwrap();
        let work = Work {
            output_list: Some(list),
            ..Default::default()
        };
        let outputs = match collect_outputs(&work) {
            Ok(o) => o,
            Err(_) => panic!("outputs should be collected"),
        };
        assert_eq!(outputs, vec![video, chat]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn only_files_from_this_job_are_ours() {
        let dir = scratch();
        let old = dir.join("old.description");
        std::fs::write(&old, "x").unwrap();
        // A file from before the job isn't ours to delete…
        let later = Touched {
            started: SystemTime::now() + Duration::from_secs(60),
            ..Default::default()
        };
        assert!(!later.is_ours(&old));
        // …one written since the job began is, and so is one that isn't there yet.
        let earlier = Touched {
            started: SystemTime::now() - Duration::from_secs(60),
            ..Default::default()
        };
        assert!(earlier.is_ours(&old));
        assert!(earlier.is_ours(&dir.join("not-yet.mp4")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_notice_reads_plainly() {
        assert_eq!(notice_text(1, 0, Some("clip.mp4")), "Done: clip.mp4");
        assert_eq!(
            notice_text(0, 1, Some("clip.mp4")),
            "Couldn't finish: clip.mp4"
        );
        assert_eq!(notice_text(3, 0, None), "All 3 jobs are done.");
        assert_eq!(notice_text(2, 1, None), "2 done, 1 couldn't be finished.");
        assert_eq!(notice_text(0, 2, None), "2 jobs couldn't be finished.");
    }
}
