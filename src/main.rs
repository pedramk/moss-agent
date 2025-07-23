use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_core::Stream;
use tokio::sync::broadcast;
use tonic::{transport::Server, Request, Response, Status};

use capture::capture_service_server::{CaptureService, CaptureServiceServer};
use capture::{Empty, Event, Status as RpcStatus};

use chrono::Local;
use rdev::{listen, Button, Event as RdevEvent, EventType, Key};
use std::cell::RefCell;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

mod system_info;
use system_info::SystemInfo;

pub mod capture {
    tonic::include_proto!("capture");
}

pub struct MyCaptureService {
    broadcaster: broadcast::Sender<Event>,
    capturing: Arc<AtomicBool>,
    system_info: Arc<Mutex<Option<SystemInfo>>>,
}

#[tonic::async_trait]
impl CaptureService for MyCaptureService {
    async fn start(&self, _: Request<Empty>) -> Result<Response<RpcStatus>, Status> {
        self.capturing.store(true, Ordering::Relaxed);
        println!("[INFO] Event capturing started");

        // Collect and send system information in a separate task
        let broadcaster = self.broadcaster.clone();
        let system_info = self.system_info.clone();
        tokio::spawn(async move {
            match SystemInfo::collect() {
                Ok(info) => {
                    let system_event = Event {
                        name: "SystemInfo".to_string(),
                        timestamp: Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
                        details: info.to_formatted_string(),
                    };

                    // Store system info for change monitoring
                    *system_info.lock().await = Some(info);

                    if let Err(e) = broadcaster.send(system_event) {
                        // Only log if it's not a "no receivers" error
                        if !e.to_string().contains("channel closed") {
                            eprintln!("[ERROR] Failed to send system info: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[ERROR] Failed to collect system info: {}", e);
                }
            }
        });

        Ok(Response::new(RpcStatus {
            message: "Started".into(),
        }))
    }

    async fn stop(&self, _: Request<Empty>) -> Result<Response<RpcStatus>, Status> {
        self.capturing.store(false, Ordering::Relaxed);
        println!("[INFO] Event capturing stopped");
        Ok(Response::new(RpcStatus {
            message: "Stopped".into(),
        }))
    }

    type StreamEventsStream =
        Pin<Box<dyn Stream<Item = Result<Event, Status>> + Send + Sync + 'static>>;

    async fn stream_events(
        &self,
        _: Request<Empty>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        let mut rx = self.broadcaster.subscribe();
        let output = async_stream::stream! {
            while let Ok(event) = rx.recv().await {
                yield Ok(event);
            }
            println!("[WARN] Event stream ended");
        };
        Ok(Response::new(Box::pin(output) as Self::StreamEventsStream))
    }
}

fn format_event_details(event: &RdevEvent) -> String {
    match &event.event_type {
        EventType::KeyPress(key) => format!("{:?}", key),
        EventType::KeyRelease(key) => format!("{:?}", key),
        EventType::MouseMove { x, y } => format!("{},{}", x, y),
        EventType::ButtonPress(button) => format!("{:?}", button),
        EventType::ButtonRelease(button) => format!("{:?}", button),
        EventType::Wheel { delta_x, delta_y } => format!("dx={},dy={}", delta_x, delta_y),
    }
}

fn compare_system_info(old: &SystemInfo, new: &SystemInfo) -> String {
    let mut changes = Vec::new();

    if old.system_info.directx_version != new.system_info.directx_version {
        changes.push(format!(
            "DirectX version changed: {} -> {}",
            old.system_info.directx_version, new.system_info.directx_version
        ));
    }

    if old.system_info.os_version != new.system_info.os_version {
        changes.push(format!(
            "OS version changed: {} -> {}",
            old.system_info.os_version, new.system_info.os_version
        ));
    }

    if old.system_info.memory_mb != new.system_info.memory_mb {
        changes.push(format!(
            "Memory changed: {} MB -> {} MB",
            old.system_info.memory_mb, new.system_info.memory_mb
        ));
    }

    if old.network_info.local_ip != new.network_info.local_ip {
        changes.push(format!(
            "Local IP changed: {} -> {}",
            old.network_info.local_ip, new.network_info.local_ip
        ));
    }

    if old.network_info.public_ip != new.network_info.public_ip {
        changes.push(format!(
            "Public IP changed: {} -> {}",
            old.network_info.public_ip, new.network_info.public_ip
        ));
    }

    // Check for USB device changes
    if old.usb_input_devices.len() != new.usb_input_devices.len() {
        changes.push(format!(
            "USB devices count changed: {} -> {}",
            old.usb_input_devices.len(),
            new.usb_input_devices.len()
        ));
    }

    // Check for monitor changes
    if old.monitors.len() != new.monitors.len() {
        changes.push(format!(
            "Monitor count changed: {} -> {}",
            old.monitors.len(),
            new.monitors.len()
        ));
    }

    // Check for video card changes
    if old.video_cards.len() != new.video_cards.len() {
        changes.push(format!(
            "Video cards count changed: {} -> {}",
            old.video_cards.len(),
            new.video_cards.len()
        ));
    }

    // Check for PCI device changes
    if old.pci_devices.len() != new.pci_devices.len() {
        changes.push(format!(
            "PCI devices count changed: {} -> {}",
            old.pci_devices.len(),
            new.pci_devices.len()
        ));
    }

    changes.join("\n")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --- Print system info on startup ---
    match system_info::SystemInfo::collect() {
        Ok(info) => {
            println!("--- System Info at Startup ---");
            println!("{}", info.to_formatted_string());
            println!("------------------------------");
        }
        Err(e) => {
            eprintln!("[ERROR] Failed to collect system info at startup: {}", e);
        }
    }
    // -------------------------------------

    let (broadcaster, _) = broadcast::channel(1024);
    let capturing = Arc::new(AtomicBool::new(false)); // Start with capturing off until client connects
    let listener_handle = Arc::new(Mutex::new(None));

    let mouse_move_interval = Arc::new(Mutex::new(0.05f64)); // in seconds

    {
        let tx = broadcaster.clone();
        let capturing_clone = Arc::clone(&capturing);
        let mouse_move_interval_clone = Arc::clone(&mouse_move_interval);

        let pressed_keys = RefCell::new(HashSet::<Key>::new());
        let pressed_buttons = RefCell::new(HashSet::<Button>::new());

        // RefCell for last MouseMove time (use Instant for precise timing)
        let last_mouse_move_time = RefCell::new(Instant::now() - Duration::from_secs(1)); // initialized to past

        let handle = std::thread::spawn(move || {
            println!("[INFO] Event listener thread ready (waiting for start command)");
            let callback = move |event: RdevEvent| {
                if !capturing_clone.load(Ordering::Relaxed) {
                    return;
                }

                let (event_name, is_new_event) = match &event.event_type {
                    EventType::KeyPress(key) => {
                        let mut keys = pressed_keys.borrow_mut();
                        if keys.contains(key) {
                            (None, false)
                        } else {
                            keys.insert(*key);
                            (Some("KeyPress"), true)
                        }
                    }
                    EventType::KeyRelease(key) => {
                        pressed_keys.borrow_mut().remove(key);
                        (Some("KeyRelease"), true)
                    }
                    EventType::ButtonPress(button) => {
                        let mut buttons = pressed_buttons.borrow_mut();
                        if buttons.contains(button) {
                            (None, false)
                        } else {
                            buttons.insert(*button);
                            (Some("MouseButtonPress"), true)
                        }
                    }
                    EventType::ButtonRelease(button) => {
                        pressed_buttons.borrow_mut().remove(button);
                        (Some("MouseButtonRelease"), true)
                    }
                    EventType::MouseMove { .. } => {
                        let now = Instant::now();
                        let mut last_time = last_mouse_move_time.borrow_mut();
                        // Read the interval (locked on each event)
                        let interval = *mouse_move_interval_clone.blocking_lock();
                        if now.duration_since(*last_time).as_secs_f64() >= interval {
                            *last_time = now;
                            (Some("MouseMove"), true)
                        } else {
                            (None, false)
                        }
                    }
                    EventType::Wheel { .. } => (Some("MouseWheel"), true),
                };

                if let Some(name) = event_name {
                    if is_new_event {
                        let now = Local::now();
                        let event_timestamp = now.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
                        let event_details = format_event_details(&event);
                        let cap_event = Event {
                            name: name.to_string(),
                            timestamp: event_timestamp,
                            details: event_details,
                        };
                        if let Err(e) = tx.send(cap_event) {
                            // Only log if it's not a "no receivers" error
                            if !e.to_string().contains("channel closed") {
                                eprintln!("[ERROR] Failed to send event: {}", e);
                            }
                        }
                    }
                }
            };

            if let Err(e) = listen(callback) {
                eprintln!("[ERROR] Error in event listener: {:?}", e);
            }
        });

        *listener_handle.lock().await = Some(handle);
    }

    // Add system monitoring thread
    {
        let tx = broadcaster.clone();
        let capturing_clone = Arc::clone(&capturing);
        tokio::spawn(async move {
            let mut last_system_info: Option<SystemInfo> = None;

            loop {
                if capturing_clone.load(Ordering::Relaxed) {
                    if let Ok(current_info) = SystemInfo::collect() {
                        if let Some(ref last_info) = last_system_info {
                            // Check for changes and send only changed values
                            let changes = compare_system_info(last_info, &current_info);
                            if !changes.is_empty() {
                                let change_event = Event {
                                    name: "SystemInfoChange".to_string(),
                                    timestamp: Local::now()
                                        .format("%Y-%m-%d %H:%M:%S%.3f")
                                        .to_string(),
                                    details: changes,
                                };

                                if let Err(e) = tx.send(change_event) {
                                    // Only log if it's not a "no receivers" error
                                    if !e.to_string().contains("channel closed") {
                                        eprintln!(
                                            "[ERROR] Failed to send system info change: {}",
                                            e
                                        );
                                    }
                                }
                            }
                        }
                        last_system_info = Some(current_info);
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });
    }

    let addr = "127.0.0.1:50051".parse()?;
    let service = MyCaptureService {
        broadcaster,
        capturing,
        system_info: Arc::new(Mutex::new(None)),
    };

    println!("[INFO] gRPC server listening on {}", addr);

    let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel();
    let server = Server::builder()
        .add_service(CaptureServiceServer::new(service))
        .serve_with_shutdown(addr, async {
            shutdown_receiver.await.ok();
            println!("[INFO] Shutting down server...");
        });

    tokio::select! {
        _ = server => {
            println!("[INFO] Server terminated");
        }
        _ = tokio::signal::ctrl_c() => {
            println!("[INFO] Received CTRL+C, shutting down...");
            shutdown_sender.send(()).ok();
        }
    }

    Ok(())
}
