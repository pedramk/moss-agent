use serde::Serialize;
use std::collections::HashMap;
use std::process::Command;
use std::str;

#[derive(Debug, Clone, Serialize)]
pub struct SystemInfo {
    pub system_info: SystemInfoCore,
    pub pci_devices: Vec<PciDevice>,
    pub drives: Vec<DriveInfo>,
    pub network_info: NetworkInfo,
    pub video_cards: Vec<VideoCard>,
    pub monitors: Vec<Monitor>,
    pub usb_input_devices: Vec<UsbDevice>,
    pub processor_info: ProcessorInfo,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemInfoCore {
    pub directx_version: String,
    pub os_version: String,
    pub real_os: String,
    pub memory_mb: u64,
    pub physical_model: String,
    pub machine_signature: String,
    pub user: String,
    pub monitor_start_time: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PciDevice {
    pub id: String,
    #[serde(rename = "type")]
    pub device_type: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessorInfo {
    pub cpu_model: String,
    pub cpu_cores: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriveInfo {
    pub serial: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkInfo {
    pub local_ip: String,
    pub public_ip: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoCard {
    pub name: String,
    pub driver_version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Monitor {
    pub model: Option<String>, // The real human-readable model, e.g., "MSI MP271A"
}

#[derive(Debug, Clone, Serialize)]
pub struct UsbDevice {
    pub name: String,
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
}

impl SystemInfo {
    pub fn collect() -> Result<Self, String> {
        Ok(SystemInfo {
            system_info: SystemInfoCore {
                directx_version: get_directx_version()?,
                os_version: get_os_info()?.0,
                real_os: get_os_info()?.1,
                memory_mb: get_memory_info()?,
                physical_model: get_physical_hardware_info()?,
                machine_signature: get_machine_signature()?,
                user: get_user_info()?,
                monitor_start_time: chrono::Utc::now().to_rfc3339(),
            },
            pci_devices: get_pci_devices()?,
            drives: get_drive_info()?,
            network_info: get_network_info()?,
            video_cards: get_video_cards()?,
            monitors: get_monitors()?,
            usb_input_devices: get_usb_devices()?,
            processor_info: get_processor_info()?,
        })
    }

    pub fn to_formatted_string(&self) -> String {
        serde_json::to_string_pretty(self)
            .unwrap_or_else(|_| "Failed to serialize system info".to_string())
    }
}

/// Helper: Parse WMIC output into a vector of hashmaps (one hashmap per device/block).
fn parse_wmic_output(output: &str) -> Vec<HashMap<String, String>> {
    let mut blocks = Vec::new();
    let mut current = HashMap::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            if !current.is_empty() {
                blocks.push(current.clone());
                current.clear();
            }
        } else if let Some((key, val)) = line.split_once('=') {
            current.insert(key.trim().to_string(), val.trim().to_string());
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

fn get_directx_version() -> Result<String, String> {
    Ok("DirectX 12".to_string())
}

fn get_pci_devices() -> Result<Vec<PciDevice>, String> {
    let output = Command::new("wmic")
        .args([
            "path",
            "win32_pnpentity",
            "where",
            "DeviceID like 'PCI%'",
            "get",
            "DeviceID,Name",
            "/format:list",
        ])
        .output()
        .map_err(|e| format!("WMIC error: {e}"))?;

    let output_str = str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;
    let blocks = parse_wmic_output(output_str);
    let mut devices = Vec::new();

    for mut block in blocks {
        let device_id = block
            .remove("DeviceID")
            .unwrap_or_else(|| "Unknown".to_string());
        let name = block.remove("Name");

        // Heuristic for device type
        let dtype = if device_id.to_lowercase().contains("display")
            || device_id.to_lowercase().contains("vga")
        {
            "display"
        } else if device_id.to_lowercase().contains("network")
            || device_id.to_lowercase().contains("ethernet")
        {
            "network"
        } else if device_id.to_lowercase().contains("storage")
            || device_id.to_lowercase().contains("ide")
            || device_id.to_lowercase().contains("sata")
        {
            "storage"
        } else {
            "unknown"
        };

        // Extract VEN/DEV
        let (mut vendor_id, mut device_id_part) = (None, None);
        if let Some(pci_part) = device_id.split('\\').find(|s| s.starts_with("VEN_")) {
            if let Some(ven_start) = pci_part.find("VEN_") {
                vendor_id = Some(pci_part[ven_start + 4..ven_start + 8].to_string());
            }
            if let Some(dev_start) = pci_part.find("DEV_") {
                device_id_part = Some(pci_part[dev_start + 4..dev_start + 8].to_string());
            }
        }
        let id = if let (Some(ven), Some(dev)) = (vendor_id, device_id_part) {
            format!("{}-{}", ven, dev)
        } else {
            device_id.clone()
        };

        devices.push(PciDevice {
            id,
            device_type: dtype.to_string(),
            name,
        });
    }
    if devices.is_empty() {
        devices.push(PciDevice {
            id: "Unknown".to_string(),
            device_type: "unknown".to_string(),
            name: None,
        });
    }
    Ok(devices)
}

fn get_memory_info() -> Result<u64, String> {
    let output = Command::new("wmic")
        .args(["computersystem", "get", "TotalPhysicalMemory", "/value"])
        .output()
        .map_err(|e| format!("WMIC error: {e}"))?;

    let output_str = str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;
    for line in output_str.lines() {
        if line.starts_with("TotalPhysicalMemory=") {
            let memory_str = line.split('=').nth(1).unwrap_or("0");
            if let Ok(memory_bytes) = memory_str.parse::<u64>() {
                return Ok(memory_bytes / 1024 / 1024);
            }
        }
    }
    Ok(16004) // Fallback
}

fn get_physical_hardware_info() -> Result<String, String> {
    let output = Command::new("wmic")
        .args([
            "computersystem",
            "get",
            "Manufacturer,Model",
            "/format:list",
        ])
        .output()
        .map_err(|e| format!("WMIC error: {e}"))?;
    let output_str = str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;

    let mut manufacturer = String::new();
    let mut model = String::new();

    for line in output_str.lines() {
        if line.starts_with("Manufacturer=") {
            manufacturer = line.split('=').nth(1).unwrap_or("").to_string();
        } else if line.starts_with("Model=") {
            model = line.split('=').nth(1).unwrap_or("").to_string();
        }
    }

    let bios_output = Command::new("wmic")
        .args(["bios", "get", "SerialNumber", "/format:list"])
        .output()
        .map_err(|e| format!("WMIC error: {e}"))?;
    let bios_str = str::from_utf8(&bios_output.stdout).map_err(|e| e.to_string())?;
    let serial = bios_str
        .lines()
        .find_map(|l| l.strip_prefix("SerialNumber=").map(|s| s.to_string()))
        .unwrap_or_default();

    Ok(format!("{} {} {}", manufacturer, model, serial))
}

fn get_os_info() -> Result<(String, String), String> {
    let output = Command::new("wmic")
        .args(["os", "get", "Version,Caption", "/format:list"])
        .output()
        .map_err(|e| format!("WMIC error: {e}"))?;
    let output_str = str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;

    let mut os_version = None;
    let mut real_os = None;
    for line in output_str.lines() {
        if line.starts_with("Version=") {
            os_version = Some(line.split('=').nth(1).unwrap_or("").to_string());
        } else if line.starts_with("Caption=") {
            real_os = Some(line.split('=').nth(1).unwrap_or("").to_string());
        }
    }
    Ok((
        os_version.unwrap_or_else(|| "Unknown".to_string()),
        real_os.unwrap_or_else(|| "Unknown Windows".to_string()),
    ))
}

fn get_machine_signature() -> Result<String, String> {
    let output = Command::new("wmic")
        .args(["csproduct", "get", "UUID", "/format:list"])
        .output()
        .map_err(|e| format!("WMIC error: {e}"))?;
    let output_str = str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;
    for line in output_str.lines() {
        if let Some(uuid) = line.strip_prefix("UUID=") {
            let uuid = uuid.trim();
            if !uuid.is_empty() && uuid != "(null)" {
                return Ok(format!("{{{}}}", uuid));
            }
        }
    }
    Ok("{Unknown-Machine-ID}".to_string())
}

fn get_user_info() -> Result<String, String> {
    let output = Command::new("whoami")
        .output()
        .map_err(|e| format!("whoami error: {e}"))?;
    let user_str = str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;
    let user = user_str.trim();
    if user.is_empty() {
        Ok("Unknown@UNKNOWN".to_string())
    } else {
        Ok(user.to_string())
    }
}

fn get_drive_info() -> Result<Vec<DriveInfo>, String> {
    let output = Command::new("wmic")
        .args(["diskdrive", "get", "SerialNumber", "/format:list"])
        .output()
        .map_err(|e| format!("WMIC error: {e}"))?;
    let output_str = str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;
    let mut drives = Vec::new();
    for line in output_str.lines() {
        if let Some(serial) = line.strip_prefix("SerialNumber=") {
            let serial = serial.trim();
            if !serial.is_empty() && serial != "(null)" {
                drives.push(DriveInfo {
                    serial: serial.to_string(),
                });
            }
        }
    }
    if drives.is_empty() {
        drives.push(DriveInfo {
            serial: "Unknown".to_string(),
        });
    }
    Ok(drives)
}

fn get_network_info() -> Result<NetworkInfo, String> {
    let output = Command::new("ipconfig")
        .output()
        .map_err(|e| format!("ipconfig error: {e}"))?;
    let output_str = str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;
    let mut local_ip = "Unknown".to_string();
    for line in output_str.lines() {
        if line.contains("IPv4 Address") || line.contains("IPv4-Adresse") {
            if let Some(ip_part) = line.split(':').nth(1) {
                let ip = ip_part.trim();
                if !ip.starts_with("127.") && !ip.starts_with("169.254.") {
                    local_ip = ip.to_string();
                    break;
                }
            }
        }
    }

    let public_ip = match Command::new("nslookup")
        .args(["myip.opendns.com", "resolver1.opendns.com"])
        .output()
    {
        Ok(output) => {
            let output_str = str::from_utf8(&output.stdout).unwrap_or("");
            let mut found_ip = "Unknown".to_string();
            for line in output_str.lines() {
                if line.starts_with("Address:") && !line.contains("#") {
                    if let Some(ip) = line.split(':').nth(1) {
                        let ip = ip.trim();
                        if ip.chars().filter(|c| *c == '.').count() == 3 {
                            found_ip = ip.to_string();
                            break;
                        }
                    }
                }
            }
            found_ip
        }
        Err(_) => "Unknown".to_string(),
    };
    Ok(NetworkInfo {
        local_ip,
        public_ip,
    })
}

fn get_video_cards() -> Result<Vec<VideoCard>, String> {
    let output = Command::new("wmic")
        .args([
            "path",
            "win32_videocontroller",
            "get",
            "Name,DriverVersion",
            "/format:list",
        ])
        .output()
        .map_err(|e| format!("WMIC error: {e}"))?;

    let output_str = str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;
    let blocks = parse_wmic_output(output_str);
    let mut cards = Vec::new();
    for block in blocks {
        let name = block
            .get("Name")
            .cloned()
            .unwrap_or_else(|| "Unknown".to_string());
        let driver_version = block.get("DriverVersion").cloned().unwrap_or_default();
        cards.push(VideoCard {
            name,
            driver_version,
        });
    }
    Ok(cards)
}

/// Uses WMI to extract monitor model info from EDID data
fn get_monitors() -> Result<Vec<Monitor>, String> {
    // First try to get monitor info from WmiMonitorID which contains EDID data
    let output = Command::new("powershell")
        .args(&[
            "-Command",
            "Get-WmiObject -Namespace root/wmi -Class WmiMonitorID | Select-Object -ExpandProperty UserFriendlyName",
        ])
        .output()
        .map_err(|e| format!("PowerShell WmiMonitorID error: {e}"))?;

    let output_str = str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;
    let mut monitors = Vec::new();

    // Parse the output - each byte is on a separate line
    let lines: Vec<&str> = output_str.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    let mut current_bytes = Vec::new();
    
    for line in lines {
        if let Ok(byte_val) = line.parse::<u8>() {
            current_bytes.push(byte_val);
            // If we hit a null terminator (0), we've reached the end of a monitor name
            if byte_val == 0 {
                if let Some(model_name) = decode_byte_array(&current_bytes) {
                    if !model_name.is_empty() && model_name != "Generic PnP Monitor" {
                        monitors.push(Monitor { model: Some(model_name) });
                    }
                }
                current_bytes.clear();
            }
        }
    }
    
    // Handle case where there's no null terminator at the end
    if !current_bytes.is_empty() {
        if let Some(model_name) = decode_byte_array(&current_bytes) {
            if !model_name.is_empty() && model_name != "Generic PnP Monitor" {
                monitors.push(Monitor { model: Some(model_name) });
            }
        }
    }

    // Fallback to desktopmonitor if WmiMonitorID didn't work
    if monitors.is_empty() {
        let fallback_output = Command::new("wmic")
            .args(&[
                "desktopmonitor",
                "get",
                "Caption",
                "/format:list",
            ])
            .output()
            .map_err(|e| format!("WMIC desktopmonitor error: {e}"))?;

        let fallback_str = str::from_utf8(&fallback_output.stdout).map_err(|e| e.to_string())?;
        let fallback_blocks = parse_wmic_output(fallback_str);

        for block in fallback_blocks {
            if let Some(caption) = block.get("Caption") {
                let model = caption.trim().to_string();
                if !model.is_empty() && model != "Default Monitor Type" {
                    monitors.push(Monitor { model: Some(model) });
                }
            }
        }
    }

    if monitors.is_empty() {
        monitors.push(Monitor {
            model: Some("Unknown".to_string()),
        });
    }
    Ok(monitors)
}

/// Decode byte array to readable text
fn decode_byte_array(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    
    // Convert bytes to string, stopping at null terminator
    let mut result = String::new();
    for &byte in bytes {
        if byte == 0 {
            break;
        }
        if byte.is_ascii() && byte >= 32 {
            result.push(byte as char);
        }
    }
    
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

// Removed decode_powershell_edid_field as it's no longer needed for WMIC output
// fn decode_powershell_edid_field(field: &str) -> String { ... }

fn get_usb_devices() -> Result<Vec<UsbDevice>, String> {
    let output = Command::new("wmic")
        .args([
            "path",
            "Win32_PnPEntity",
            "where",
            "DeviceID like 'USB%'",
            "get",
            "Name,DeviceID,Description",
            "/format:list",
        ])
        .output()
        .map_err(|e| format!("WMIC error: {e}"))?;
    let output_str = str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;

    let blocks = parse_wmic_output(output_str);
    let mut devices = Vec::new();

    for block in blocks {
        let name = block
            .get("Name")
            .cloned()
            .unwrap_or_else(|| "USB Device".to_string());
        let device_id = block.get("DeviceID").cloned().unwrap_or_default();

        let (mut vendor_id, mut product_id) = (None, None);
        if let Some(vid_start) = device_id.find("VID_") {
            vendor_id = device_id
                .get(vid_start + 4..vid_start + 8)
                .map(|s| s.to_string());
        }
        if let Some(pid_start) = device_id.find("PID_") {
            product_id = device_id
                .get(pid_start + 4..pid_start + 8)
                .map(|s| s.to_string());
        }

        devices.push(UsbDevice {
            name,
            vendor_id,
            product_id,
        });
    }
    Ok(devices)
}

fn get_processor_info() -> Result<ProcessorInfo, String> {
    let output = Command::new("wmic")
        .args(["cpu", "get", "Name,MaxClockSpeed", "/format:list"])
        .output()
        .map_err(|e| format!("WMIC error: {e}"))?;
    let output_str = str::from_utf8(&output.stdout).map_err(|e| e.to_string())?;
    let blocks = parse_wmic_output(output_str);

    let mut cpu_name = String::new();
    for block in blocks {
        cpu_name = block.get("Name").cloned().unwrap_or_default();
        break;
    }

    let core_output = Command::new("wmic")
        .args(["cpu", "get", "NumberOfLogicalProcessors", "/format:list"])
        .output()
        .map_err(|e| format!("WMIC error: {e}"))?;
    let core_str = str::from_utf8(&core_output.stdout).map_err(|e| e.to_string())?;
    let mut cpu_cores = 0;
    for line in core_str.lines() {
        if let Some(val) = line.strip_prefix("NumberOfLogicalProcessors=") {
            if let Ok(cores) = val.trim().parse::<u32>() {
                cpu_cores = cores;
                break;
            }
        }
    }
    Ok(ProcessorInfo {
        cpu_model: cpu_name,
        cpu_cores,
    })
}
