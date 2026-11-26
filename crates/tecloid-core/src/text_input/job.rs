//! Lifetime, destination and completion accounting for asynchronous text input.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use super::timing::{MAX_TEXT_BYTES, TextTiming};
use crate::events::TextInputFailure;

/// How long text input waits for the target window to take the focus.
const TARGET_TIMEOUT: Duration = Duration::from_millis(500);
const TARGET_POLL: Duration = Duration::from_millis(5);

const MAX_QUEUED_TEXTS: usize = 16;

#[derive(Default)]
struct Progress {
    generation: u64,
    active: bool,
    pending: usize,
    capture_registered: bool,
    capture_suspended: bool,
    permissions: usize,
}

static PROGRESS: Mutex<Progress> = Mutex::new(Progress {
    generation: 0,
    active: false,
    pending: 0,
    capture_registered: false,
    capture_suspended: false,
    permissions: 0,
});

/// Completion and cancellation of one macro text step, independent of other input.
#[derive(Clone, Debug, Default)]
pub struct MacroText(Arc<MacroTextState>);

#[derive(Debug, Default)]
struct MacroTextState {
    cancelled: AtomicBool,
    result: Mutex<Option<Result<(), ()>>>,
}

impl MacroText {
    pub(crate) fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn result(&self) -> Option<Result<(), ()>> {
        *self.0.result.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn complete(&self, failed: bool) {
        let mut result = self.0.result.lock().unwrap_or_else(|e| e.into_inner());
        if result.is_none() {
            *result = Some(if failed { Err(()) } else { Ok(()) });
        }
    }
}

#[derive(Clone)]
pub(crate) struct TextRequest {
    macro_job: Option<MacroText>,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    popup_choice: Option<Instant>,
    completed: Arc<AtomicBool>,
    generation: u64,
    timing: Arc<Mutex<TextTiming>>,
    target: Arc<crate::active_window::TextTarget>,
}

impl Progress {
    fn reserve(&mut self, text_len: usize) -> Result<u64, TextInputFailure> {
        if !self.active {
            return Err(unavailable("text input session is stopped"));
        }
        if text_len > MAX_TEXT_BYTES || self.pending >= MAX_QUEUED_TEXTS {
            return Err(unavailable("text input queue or text size limit exceeded"));
        }
        self.pending += 1;
        Ok(self.generation)
    }

    fn complete(&mut self, generation: u64) {
        if self.generation == generation {
            self.pending = self.pending.saturating_sub(1);
        }
    }

    fn reset(&mut self, active: bool) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = 0;
        self.permissions = 0;
        self.capture_suspended = false;
        self.active = active;
    }
}

impl TextRequest {
    pub(crate) fn enqueue(text: &str) -> Result<Self, TextInputFailure> {
        Self::enqueue_inner(
            text,
            None,
            crate::active_window::TextTarget::capture(),
            None,
        )
    }

    pub(crate) fn enqueue_macro(text: &str, job: MacroText) -> Result<Self, TextInputFailure> {
        if job.cancelled() {
            return Err(unavailable("macro text input cancelled"));
        }
        let result = Self::enqueue_inner(
            text,
            Some(job.clone()),
            crate::active_window::TextTarget::capture(),
            None,
        );
        if result.is_err() {
            job.complete(true);
        }
        result
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn enqueue_popup(
        text: &str,
        target: crate::active_window::TextTarget,
        chosen: Instant,
    ) -> Result<Self, TextInputFailure> {
        Self::enqueue_inner(text, None, target, Some(chosen))
    }

    fn enqueue_inner(
        text: &str,
        macro_job: Option<MacroText>,
        target: crate::active_window::TextTarget,
        popup_choice: Option<Instant>,
    ) -> Result<Self, TextInputFailure> {
        let timing = TextTiming::new(text).map_err(|e| unavailable(&e))?;
        let mut progress = PROGRESS.lock().unwrap_or_else(|e| e.into_inner());
        let generation = progress.reserve(text.len())?;
        let request = Self {
            macro_job,
            popup_choice,
            completed: Arc::new(AtomicBool::new(false)),
            generation,
            timing: Arc::new(Mutex::new(timing)),
            target: Arc::new(target),
        };
        Ok(request)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn gnome_window(&self) -> Result<&str, TextInputFailure> {
        match self.target.as_ref() {
            crate::active_window::TextTarget::Gnome(id, _) => Ok(id),
            _ => Err(TextInputFailure::DestinationChanged),
        }
    }

    pub(crate) fn check(&self) -> Result<(), TextInputFailure> {
        self.check_lifetime()?;
        if !self.target.is_current() {
            return Err(TextInputFailure::DestinationChanged);
        }
        Ok(())
    }

    pub(crate) fn prepare(&self) -> Result<(), TextInputFailure> {
        self.check_lifetime()?;
        self.timing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .prepare()
            .map_err(|e| unavailable(&e))
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn await_permission(
        &self,
        timeout: Duration,
    ) -> Result<Permission, TextInputFailure> {
        self.check_lifetime()?;
        self.timing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .await_permission(timeout)
            .map_err(|e| unavailable(&e))?;
        {
            let mut progress = PROGRESS.lock().unwrap_or_else(|e| e.into_inner());
            if progress.generation != self.generation {
                return Err(unavailable("text input cancelled"));
            }
            progress.permissions += 1;
        }
        let permission = Permission(self.clone());
        loop {
            self.check_lifetime()?;
            let ready = {
                let progress = PROGRESS.lock().unwrap_or_else(|e| e.into_inner());
                !progress.capture_registered || progress.capture_suspended
            };
            if ready {
                return Ok(permission);
            }
            std::thread::sleep(TARGET_POLL);
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn wait_for_capture(&self) -> Result<(), TextInputFailure> {
        loop {
            self.check_lifetime()?;
            if !PROGRESS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .capture_suspended
            {
                return Ok(());
            }
            std::thread::sleep(TARGET_POLL);
        }
    }

    pub(crate) fn start_typing(&self) -> Result<(), TextInputFailure> {
        self.check()?;
        self.timing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .start_typing()
            .map_err(|e| unavailable(&e))
    }

    pub(crate) fn mark_progress(&self) {
        self.timing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .mark_progress();
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn start_tool(&self, timeout: Duration) -> Result<(), TextInputFailure> {
        self.check()?;
        self.timing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .start_tool(timeout)
            .map_err(|e| unavailable(&e))
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn is_terminal(&self) -> bool {
        self.target.is_terminal()
    }

    pub(crate) fn wait_for_target(&self) -> Result<(), TextInputFailure> {
        let deadline = Instant::now() + TARGET_TIMEOUT;
        loop {
            self.check_lifetime()?;
            if self.target.is_current() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return self.check();
            }
            std::thread::sleep(TARGET_POLL);
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn wait_for_initial_target(&self) -> Result<(), TextInputFailure> {
        let Some(chosen) = self.popup_choice else {
            return self.wait_for_target();
        };
        loop {
            self.check_lifetime()?;
            if chosen.elapsed() >= TARGET_TIMEOUT || self.target.popup_target_changed() {
                return Err(TextInputFailure::DestinationChanged);
            }
            if self.target.is_current() {
                log::debug!(
                    "[text] popup choice to verified target: {:?}",
                    chosen.elapsed()
                );
                return Ok(());
            }
            self.target.wait_for_popup_focus_update(TARGET_POLL);
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn popup_elapsed(&self) -> Option<Duration> {
        self.popup_choice.map(|chosen| chosen.elapsed())
    }

    pub(crate) fn check_lifetime(&self) -> Result<(), TextInputFailure> {
        if self.macro_job.as_ref().is_some_and(MacroText::cancelled) {
            return Err(unavailable("macro text input cancelled"));
        }
        let progress = PROGRESS.lock().unwrap_or_else(|e| e.into_inner());
        if self.generation != progress.generation {
            return Err(unavailable("text input cancelled"));
        }
        self.timing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .check()
            .map_err(|e| unavailable(&e))
    }

    pub(crate) fn complete(&self, failed: bool) {
        if self.completed.swap(true, Ordering::AcqRel) {
            return;
        }
        let mut progress = PROGRESS.lock().unwrap_or_else(|e| e.into_inner());
        progress.complete(self.generation);
        if let Some(job) = &self.macro_job {
            job.complete(failed);
        }
    }
}

/// Keeps physical capture suspended for the complete portal consent exchange.
#[cfg(target_os = "linux")]
pub(crate) struct Permission(TextRequest);

#[cfg(target_os = "linux")]
impl Drop for Permission {
    fn drop(&mut self) {
        let mut progress = PROGRESS.lock().unwrap_or_else(|e| e.into_inner());
        if progress.generation == self.0.generation {
            progress.permissions = progress.permissions.saturating_sub(1);
        }
        drop(progress);
        self.0
            .timing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .permission_finished();
    }
}

pub(crate) fn cancel() {
    let mut progress = PROGRESS.lock().unwrap_or_else(|e| e.into_inner());
    progress.reset(false);
}

/// Owns one mapper text session, including cancellation on unwinding.
pub(crate) struct Session {
    generation: u64,
}

impl Session {
    pub(crate) fn start() -> Self {
        let mut progress = PROGRESS.lock().unwrap_or_else(|e| e.into_inner());
        progress.reset(true);
        progress.capture_registered = false;
        Self {
            generation: progress.generation,
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn register_capture(&self) {
        PROGRESS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .capture_registered = true;
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn permission_pending(&self) -> bool {
        PROGRESS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .permissions
            != 0
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn capture_suspended(&self, suspended: bool) {
        PROGRESS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .capture_suspended = suspended;
    }

    pub(crate) fn restart(&mut self) {
        let mut progress = PROGRESS.lock().unwrap_or_else(|e| e.into_inner());
        progress.reset(true);
        self.generation = progress.generation;
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let mut progress = PROGRESS.lock().unwrap_or_else(|e| e.into_inner());
        if progress.generation == self.generation {
            progress.reset(false);
            progress.capture_registered = false;
        }
    }
}

pub(crate) fn pending() -> bool {
    PROGRESS.lock().unwrap_or_else(|e| e.into_inner()).pending != 0
}

fn unavailable(message: &str) -> TextInputFailure {
    TextInputFailure::BackendUnavailable(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ownership_survives_restarts_and_cancels_on_unwind() {
        let old = Session::start();
        let mut current = Session::start();
        drop(old);
        assert!(PROGRESS.lock().unwrap().active);
        let job = MacroText::default();
        let request = TextRequest::enqueue_macro("macro text", job.clone()).unwrap();
        let unrelated = TextRequest::enqueue("other text").unwrap();
        #[cfg(target_os = "linux")]
        {
            current.register_capture();
            unrelated.prepare().unwrap();
            let handshake = unrelated.clone();
            let worker =
                std::thread::spawn(move || handshake.await_permission(Duration::from_secs(180)));
            let deadline = Instant::now() + Duration::from_secs(2);
            while !current.permission_pending() {
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            assert!(
                !worker.is_finished(),
                "dialog cannot open before capture is released"
            );
            current.capture_suspended(true);
            let permission = worker.join().unwrap().unwrap();
            assert!(current.permission_pending());
            drop(permission);
            assert!(!current.permission_pending());
            current.capture_suspended(false);
            unrelated.wait_for_capture().unwrap();
        }

        job.cancel();
        assert!(request.check_lifetime().is_err());
        assert!(unrelated.check_lifetime().is_ok());
        request.complete(true);
        request.complete(true);
        assert_eq!(PROGRESS.lock().unwrap().pending, 1);
        unrelated.complete(false);
        assert!(!pending());
        assert!(TextRequest::enqueue_macro("cancelled", job).is_err());
        #[cfg(target_os = "linux")]
        {
            let cancelled = TextRequest::enqueue("cancelled consent").unwrap();
            cancelled.prepare().unwrap();
            let worker =
                std::thread::spawn(move || cancelled.await_permission(Duration::from_secs(180)));
            let deadline = Instant::now() + Duration::from_secs(2);
            while !current.permission_pending() {
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            current.restart();
            assert!(worker.join().unwrap().is_err());
            assert!(!current.permission_pending());
        }
        current.restart();
        drop(current);
        assert!(!PROGRESS.lock().unwrap().active);
        assert!(
            std::panic::catch_unwind(|| {
                let _session = Session::start();
                panic!("mapper failed");
            })
            .is_err()
        );
        assert!(!PROGRESS.lock().unwrap().active);
    }

    #[test]
    fn old_completions_cannot_release_new_jobs() {
        let mut progress = Progress {
            active: true,
            ..Progress::default()
        };
        let old = progress.reserve(10).unwrap();
        progress.reset(false);
        assert!(progress.reserve(1).is_err());
        progress.reset(true);
        let new = progress.reserve(10).unwrap();
        progress.complete(old);
        assert_eq!(progress.pending, 1);
        progress.complete(new);
        assert_eq!(progress.pending, 0);
    }

    #[test]
    fn queue_and_payload_limits_fail_without_reserving_extra_jobs() {
        let mut progress = Progress {
            active: true,
            ..Progress::default()
        };
        assert!(progress.reserve(MAX_TEXT_BYTES + 1).is_err());
        assert_eq!(progress.pending, 0);
        for _ in 0..MAX_QUEUED_TEXTS {
            progress.reserve(1).unwrap();
        }
        assert!(progress.reserve(1).is_err());
        assert_eq!(progress.pending, MAX_QUEUED_TEXTS);
    }
}
