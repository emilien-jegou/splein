// Responsibility: AppExtension coordinating the code-swap loop and window chrome updates.

use super::builder::BuildRunner;
use super::chrome::HotWindowInfo;
use super::contract::LoadedUiContract;
use super::events::SupervisorEvent;
use super::supervisor::DevSupervisor;
use super::window::ComponentRootFn;
use crate::app::extension::AppExtension;
use crate::foundation::Color;
use crate::runtime::Engine;
use std::cell::Cell;
use std::rc::Rc;
use tracing::instrument;

pub struct ComponentReloadExtension {
    supervisor: DevSupervisor,
    shared_root: Rc<Cell<Option<ComponentRootFn>>>,
    pending_chrome: Option<(HotWindowInfo, String)>,
    applied_chrome: Option<(HotWindowInfo, String)>,
}

impl ComponentReloadExtension {
    pub fn new(runner: Box<dyn BuildRunner>) -> Self {
        Self {
            supervisor: DevSupervisor::new(runner),
            shared_root: Rc::new(Cell::new(None)),
            pending_chrome: None,
            applied_chrome: None,
        }
    }

    #[instrument(skip_all)]
    fn apply_library_swap(&mut self, engine: &mut Engine) -> bool {
        let contract = match LoadedUiContract::resolve(self.supervisor.loader()) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("riode: failed to activate hot library: {e}");
                return true;
            }
        };

        self.pending_chrome = Some((contract.window_info, contract.window_title));

        // Inside ComponentReloadExtension::apply_library_swap:

        self.shared_root.set(Some(contract.root));
        let shared = Rc::clone(&self.shared_root);
        engine.set_root_factory(Rc::new(move || {
            shared.get().expect("root installed")()
        }));

        engine.queue_full_reload();
        tracing::info!("riode: hot-reload code swap installed at {:p}", contract.root as *const ());
        true
    }
}

impl AppExtension for ComponentReloadExtension {
    fn on_idle(&mut self) -> bool {
        matches!(
            self.supervisor.poll(),
            SupervisorEvent::LibraryReady
                | SupervisorEvent::Building
                | SupervisorEvent::BuildSettled
        )
    }

    #[cfg(feature = "desktop")]
    fn on_window(&mut self, window: &winit::window::Window, engine: &mut Engine) {
        let Some((info, title)) = self.pending_chrome.take() else {
            return;
        };
        if self.applied_chrome.as_ref() != Some(&(info, title.clone())) {
            let clamped = info.with_clamped_size(1, 1);
            engine.set_background(Color::hex(clamped.background));
            window.set_title(&title);
            let _ = window.request_inner_size(winit::dpi::LogicalSize::new(
                clamped.width,
                clamped.height,
            ));
            window.set_visible(true);
            self.applied_chrome = Some((info, title));
        }
    }

    fn on_update(&mut self, engine: &mut Engine) -> bool {
        match self.supervisor.poll() {
            SupervisorEvent::LibraryReady => self.apply_library_swap(engine),
            SupervisorEvent::BuildSettled | SupervisorEvent::Building => true,
            SupervisorEvent::BuildFailed { cause, log } => {
                engine.hot_reload_mut().record_build_failure(cause, log);
                true
            }
            SupervisorEvent::Idle => false,
        }
    }
}
