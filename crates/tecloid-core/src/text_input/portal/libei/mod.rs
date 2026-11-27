//! The libei backend: typing through an EIS connection from the portal.

mod keys;

use super::*;
use crate::text_input::portal::xkb::KeyTable;

pub(super) struct LibeiBackend {
    clipboard: ClipboardAccess,
    conn: Connection,
    session_handle: OwnedObjectPath,
    context: ei::Context,
    converter: EiEventConverter,
    connection: reis::event::Connection,
    capability: DeviceCapability,
    text_device: Option<(ei::Device, ei::Text)>,
    keyboard_device: Option<(ei::Device, ei::Keyboard)>,
    keyboard_emulating: bool,
    sequence: u32,
    /// XKB keymap text of the keyboard device, as the compositor sent it.
    keymap: Option<Vec<u8>>,
    /// Active layout group the compositor reported.
    group: Option<u32>,
    /// Key table built from `keymap` for a group.
    table: Option<((u32, u32), Arc<KeyTable>)>,
    locked: u32,
    depressed: u32,
    latched: u32,
}

impl LibeiBackend {
    pub(super) fn new(request: &TextRequest, capability: DeviceCapability) -> Result<Self, String> {
        let (conn, session_handle, clipboard) = init_portal(request)?;
        let portal = match Proxy::new(&conn, PORTAL_DEST, PORTAL_PATH, IFACE_REMOTE) {
            Ok(portal) => portal,
            Err(e) => {
                close_session(&conn, &session_handle);
                return Err(format!("create RemoteDesktop proxy for EIS: {e}"));
            }
        };
        let empty: HashMap<String, Value> = HashMap::new();
        let fd: OwnedFd = match portal.call("ConnectToEIS", &(&session_handle, empty)) {
            Ok(fd) => fd,
            Err(e) => {
                close_session(&conn, &session_handle);
                return Err(format!("ConnectToEIS: {e}"));
            }
        };
        let socket = UnixStream::from(std::os::fd::OwnedFd::from(fd));
        let context = match ei::Context::new(socket) {
            Ok(context) => context,
            Err(e) => {
                close_session(&conn, &session_handle);
                return Err(format!("create EI context: {e}"));
            }
        };
        let handshake = match ei_handshake_with_timeout(&context, EI_SETUP_TIMEOUT) {
            Ok(handshake) => handshake,
            Err(e) => {
                close_session(&conn, &session_handle);
                return Err(e);
            }
        };
        let converter = EiEventConverter::new(&context, handshake);
        let connection = converter.connection().clone();
        let mut backend = Self {
            clipboard,
            conn,
            session_handle,
            context,
            converter,
            connection,
            capability,
            text_device: None,
            keyboard_device: None,
            keyboard_emulating: false,
            sequence: 1,
            keymap: None,
            group: None,
            table: None,
            locked: 0,
            depressed: 0,
            latched: 0,
        };
        let ready = if capability == DeviceCapability::Text {
            |backend: &Self| backend.text_device.is_some()
        } else {
            Self::has_keyboard
        };
        if let Err(e) = backend.wait_until(EI_SETUP_TIMEOUT, ready) {
            close_session(&backend.conn, &backend.session_handle);
            return Err(e);
        }
        Ok(backend)
    }

    pub(super) fn commit_text(
        &mut self,
        text: &str,
        request: &TextRequest,
    ) -> Result<(), TextInputFailure> {
        self.pump_events()
            .map_err(TextInputFailure::BackendUnavailable)?;
        let (device, input) = self.text_device.as_ref().ok_or_else(|| {
            TextInputFailure::BackendUnavailable("EIS text device is not ready".into())
        })?;
        request.start_typing()?;
        let serial = self.connection.serial();
        device.start_emulating(serial, self.sequence);
        self.sequence = self.sequence.wrapping_add(1);
        if let Some(elapsed) = request.popup_elapsed() {
            log::debug!("[text] popup choice to EIS text send: {elapsed:?}");
        }
        input.utf8(text);
        device.frame(serial, monotonic_micros());
        device.stop_emulating(serial);
        self.synchronize(request)
            .map_err(TextInputFailure::BackendUnavailable)
    }

    fn synchronize(&mut self, request: &TextRequest) -> Result<(), String> {
        let done = Arc::new(AtomicBool::new(false));
        let completed = done.clone();
        let callback = self.connection.connection().sync(1);
        self.converter
            .add_callback_handler(callback, move |_| completed.store(true, Ordering::SeqCst));
        self.context
            .flush()
            .map_err(|e| format!("EI sync flush: {e}"))?;
        let deadline = Instant::now() + EI_ACK_TIMEOUT;
        while !done.load(Ordering::SeqCst) {
            request.check().map_err(|e| format!("{e:?}"))?;
            self.pump_events()?;
            if Instant::now() >= deadline {
                return Err("EI input acknowledgement timed out".into());
            }
            if !done.load(Ordering::SeqCst) {
                thread::sleep(EI_ACK_POLL_INTERVAL);
            }
        }
        Ok(())
    }

    fn wait_until(&mut self, timeout: Duration, ready: fn(&Self) -> bool) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            self.pump_events()?;
            if ready(self) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(if self.capability == DeviceCapability::Text {
                    "The portal did not provide an EIS text device"
                } else {
                    "EI keyboard device is not available"
                }
                .into());
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn pump_events(&mut self) -> Result<(), String> {
        self.context.read().map_err(|e| format!("EI read: {e}"))?;
        while let Some(result) = self.context.pending_event() {
            let event = match result {
                reis::PendingRequestResult::Request(event) => event,
                reis::PendingRequestResult::ParseError(e) => {
                    return Err(format!("EI parse error: {e}"));
                }
                reis::PendingRequestResult::InvalidObject(id) => {
                    return Err(format!("EI invalid object: {id}"));
                }
            };
            self.converter
                .handle_event(event)
                .map_err(|e| format!("EI event error: {e}"))?;
        }
        while let Some(event) = self.converter.next_event() {
            self.handle_event(event)?;
        }
        self.context.flush().map_err(|e| format!("EI flush: {e}"))?;
        Ok(())
    }

    fn is_keyboard(&self, device: &ei::Device) -> bool {
        self.keyboard_device
            .as_ref()
            .is_some_and(|(keyboard, _)| keyboard == device)
    }

    fn handle_event(&mut self, event: EiEvent) -> Result<(), String> {
        match event {
            EiEvent::Disconnected(e) => {
                return Err(format!("EI disconnected: {:?}", e.explanation));
            }
            EiEvent::SeatAdded(e) => {
                e.seat.bind_capabilities(BitFlags::from(self.capability));
            }
            EiEvent::DeviceAdded(e) => {
                if let Some(keyboard) = e.device.interface::<ei::Keyboard>() {
                    let device = e.device.device().clone();
                    self.keymap = e.device.keymap().and_then(read_keymap);
                    self.table = None;
                    self.group = None;
                    self.locked = 0;
                    self.depressed = 0;
                    self.latched = 0;
                    self.keyboard_emulating = false;
                    self.start_keyboard_emulation(&device);
                    self.keyboard_device = Some((device, keyboard));
                }
            }
            EiEvent::KeyboardModifiers(e) if self.is_keyboard(e.device.device()) => {
                self.group = Some(e.group);
                self.locked = e.locked;
                self.depressed = e.depressed;
                self.latched = e.latched;
            }
            EiEvent::DevicePaused(e) if self.is_keyboard(e.device.device()) => {
                self.keyboard_emulating = false;
                self.depressed = 0;
                self.latched = 0;
                self.locked = 0;
                self.group = None;
                self.table = None;
            }
            EiEvent::DeviceResumed(e) if self.capability == DeviceCapability::Text => {
                if let Some(text) = e.device.interface::<ei::Text>() {
                    self.text_device = Some((e.device.device().clone(), text));
                }
            }
            EiEvent::DevicePaused(e) if self.is_text(e.device.device()) => {
                self.text_device = None;
            }
            EiEvent::DeviceRemoved(e) if self.is_text(e.device.device()) => {
                self.text_device = None;
            }
            EiEvent::DeviceResumed(e) if self.is_keyboard(e.device.device()) => {
                self.start_keyboard_emulation(e.device.device());
            }
            EiEvent::DeviceRemoved(e) if self.is_keyboard(e.device.device()) => {
                self.keyboard_device = None;
                self.keyboard_emulating = false;
                self.keymap = None;
                self.table = None;
                self.group = None;
                self.locked = 0;
                self.depressed = 0;
                self.latched = 0;
            }
            _ => {}
        }
        Ok(())
    }

    fn is_text(&self, device: &ei::Device) -> bool {
        self.text_device
            .as_ref()
            .is_some_and(|(text, _)| text == device)
    }

    fn has_keyboard(&self) -> bool {
        self.keyboard_device.is_some()
    }

    fn start_keyboard_emulation(&mut self, device: &ei::Device) {
        if self.keyboard_emulating {
            return;
        }
        device.start_emulating(self.connection.serial(), self.sequence);
        self.sequence += 1;
        self.keyboard_emulating = true;
    }

    pub(super) fn close(self) {
        if !matches!(self.clipboard, ClipboardAccess::Portal(_)) {
            close_session(&self.conn, &self.session_handle);
        }
    }
}

/// Read the XKB keymap an EI keyboard device carries.
fn read_keymap(keymap: &reis::event::Keymap) -> Option<Vec<u8>> {
    if keymap.type_ != ei::keyboard::KeymapType::Xkb {
        return None;
    }
    let file = std::fs::File::from(keymap.fd.try_clone().ok()?);
    let mut text = vec![0; keymap.size as usize];
    match file.read_exact_at(&mut text, 0) {
        Ok(()) => Some(text),
        Err(e) => {
            log::warn!("[libei] cannot read the device keymap: {e}");
            None
        }
    }
}

fn monotonic_micros() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let ok = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) } == 0;
    if ok {
        (ts.tv_sec as u64 * 1_000_000) + (ts.tv_nsec as u64 / 1_000)
    } else {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }
}

fn ei_handshake_with_timeout(
    context: &ei::Context,
    timeout: Duration,
) -> Result<reis::handshake::HandshakeResp, String> {
    let mut handshaker =
        reis::handshake::EiHandshaker::new(crate::storage::APP_NAME, ContextType::Sender);
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        context
            .read()
            .map_err(|e| format!("EI handshake read: {e}"))?;
        while let Some(result) = context.pending_event() {
            let event = match result {
                reis::PendingRequestResult::Request(event) => event,
                reis::PendingRequestResult::ParseError(e) => {
                    return Err(format!("EI handshake parse error: {e}"));
                }
                reis::PendingRequestResult::InvalidObject(id) => {
                    return Err(format!("EI handshake invalid object: {id}"));
                }
            };
            if let Some(handshake) = handshaker
                .handle_event(event)
                .map_err(|e| format!("EI handshake: {e}"))?
            {
                return Ok(handshake);
            }
        }
        context
            .flush()
            .map_err(|e| format!("EI handshake flush: {e}"))?;
        thread::sleep(POLL_INTERVAL);
    }
    Err("EI handshake timed out".into())
}
