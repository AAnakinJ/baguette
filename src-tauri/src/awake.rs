//! Keeps the computer from going to sleep while the night pilot works (the screen may
//! still turn off). Released when dropped.

pub struct KeepAwake {
    #[cfg(windows)]
    stop: Option<std::sync::mpsc::Sender<()>>,
    #[cfg(not(windows))]
    child: Option<std::process::Child>,
}

impl KeepAwake {
    pub fn start() -> Self {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
            // The request belongs to the thread that makes it: a dedicated thread holds it.
            let (tx, rx) = std::sync::mpsc::channel::<()>();
            std::thread::spawn(move || {
                unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
                let _ = rx.recv();
                unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
            });
            KeepAwake { stop: Some(tx) }
        }
        #[cfg(target_os = "linux")]
        {
            let child = std::process::Command::new("systemd-inhibit")
                .args(["--what=idle:sleep", "--mode=block", "--who", baguette_core::product::PRODUCT_NAME, "--why", "Night pilot", "sleep", "infinity"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .ok();
            KeepAwake { child }
        }
        #[cfg(target_os = "macos")]
        {
            let child = std::process::Command::new("caffeinate").args(["-i", "-w", &std::process::id().to_string()]).spawn().ok();
            KeepAwake { child }
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            KeepAwake { child: None }
        }
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        #[cfg(not(windows))]
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}
