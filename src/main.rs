use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_core::Stream;
use tokio::sync::broadcast;
use tonic::{transport::Server, Request, Response, Status};

use capture::capture_service_server::{CaptureService, CaptureServiceServer};
use capture::{Empty, Event, Status as RpcStatus};

use rdev::{listen, EventType, Event as RdevEvent, Key, Button};
use chrono::Local;
use tokio::sync::Mutex;
use std::collections::HashSet;
use std::cell::RefCell;
use std::time::{Duration, Instant};

pub mod capture {
    tonic::include_proto!("capture");
}

pub struct MyCaptureService {
    broadcaster: broadcast::Sender<Event>,
    capturing: Arc<AtomicBool>,
}

#[tonic::async_trait]
impl CaptureService for MyCaptureService {
    async fn start(&self, _: Request<Empty>) -> Result<Response<RpcStatus>, Status> {
        self.capturing.store(true, Ordering::Relaxed);
        println!("[INFO] Event capturing started");
        Ok(Response::new(RpcStatus { message: "Started".into() }))
    }

    async fn stop(&self, _: Request<Empty>) -> Result<Response<RpcStatus>, Status> {
        self.capturing.store(false, Ordering::Relaxed);
        println!("[INFO] Event capturing stopped");
        Ok(Response::new(RpcStatus { message: "Stopped".into() }))
    }

    type StreamEventsStream = Pin<Box<dyn Stream<Item = Result<Event, Status>> + Send + Sync + 'static>>;

    async fn stream_events(&self, _: Request<Empty>) -> Result<Response<Self::StreamEventsStream>, Status> {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (broadcaster, _) = broadcast::channel(1024);
    let capturing = Arc::new(AtomicBool::new(true)); // Start with capturing on
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
                            eprintln!("[ERROR] Failed to send event: {}", e);
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

    let addr = "127.0.0.1:50051".parse()?;
    let service = MyCaptureService {
        broadcaster,
        capturing,
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
