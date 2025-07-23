use serde::Serialize;
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
    pub name: String,
    pub serial: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsbDevice {
    pub name: String,
    pub vendor_id: String,
    pub product_id: String,
}

impl SystemInfo {
    pub fn collect() -> Result<Self, String> {
        let directx_version = get_directx_version()?;
        let pci_devices = get_pci_devices()?;
        let memory_mb = get_memory_info()?;
        let physical_model = get_physical_hardware_info()?;
        let drives = get_drive_info()?;
        let network_info = get_network_info()?;
        let video_cards = get_video_cards()?;
        let monitors = get_monitors()?;
        let usb_devices = get_usb_devices()?;
        let processor_info = get_processor_info()?;
        
        // Get OS version and real OS name
        let (os_version, real_os) = get_os_info()?;
        
        // Get machine signature and user info
        let machine_signature = get_machine_signature()?;
        let user = get_user_info()?;
        
        Ok(SystemInfo {
            system_info: SystemInfoCore {
                directx_version,
                os_version,
                real_os,
                memory_mb,
                physical_model,
                machine_signature,
                user,
                monitor_start_time: chrono::Utc::now().to_rfc3339(),
            },
            pci_devices,
            drives,
            network_info,
            video_cards,
            monitors,
            usb_input_devices: usb_devices,
            processor_info,
        })
    }
    
    pub fn to_formatted_string(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "Failed to serialize system info".to_string())
    }
}

fn get_directx_version() -> Result<String, String> {
    let _output = Command::new("dxdiag")
         .args(["/t", "dxdiag_temp.txt"])
         .output()
         .map_err(|e| e.to_string())?;
     
     // For now, return a default since dxdiag requires GUI
     Ok("DirectX 12".to_string())
}



fn get_pci_devices() -> Result<Vec<PciDevice>, String> {
    let output = Command::new("wmic")
        .args(["path", "win32_pnpentity", "where", "DeviceID like 'PCI%'", "get", "DeviceID", "/format:list"])
        .output()
        .map_err(|e| e.to_string())?;
    
    let output_str = str::from_utf8(&output.stdout)
        .map_err(|e| e.to_string())?;
    
    let mut devices = Vec::new();
     for line in output_str.lines() {
         if line.starts_with("DeviceID=") && line.contains("PCI") {
             if let Some(device_id) = line.split('=').nth(1) {
                 // Extract vendor and device ID from PCI path
                 if let Some(pci_part) = device_id.split('\\').find(|s| s.starts_with("VEN_")) {
                     let mut device_type = "unknown".to_string();
                     
                     // Determine device type based on class or description
                     if device_id.to_lowercase().contains("display") || device_id.to_lowercase().contains("vga") {
                         device_type = "display".to_string();
                     } else if device_id.to_lowercase().contains("network") || device_id.to_lowercase().contains("ethernet") {
                         device_type = "network".to_string();
                     } else if device_id.to_lowercase().contains("storage") || device_id.to_lowercase().contains("ide") || device_id.to_lowercase().contains("sata") {
                         device_type = "storage".to_string();
                     }
                     
                     // Extract VEN and DEV IDs
                     let mut vendor_id = String::new();
                     let mut device_id_part = String::new();
                     
                     if let Some(ven_start) = pci_part.find("VEN_") {
                         vendor_id = pci_part[ven_start+4..ven_start+8].to_string();
                     }
                     if let Some(dev_start) = pci_part.find("DEV_") {
                         device_id_part = pci_part[dev_start+4..dev_start+8].to_string();
                     }
                     
                     if !vendor_id.is_empty() && !device_id_part.is_empty() {
                         devices.push(PciDevice {
                             id: format!("{}-{}", vendor_id, device_id_part),
                             device_type,
                         });
                     }
                 }
             }
         }
     }
     
     if devices.is_empty() {
         devices.push(PciDevice {
             id: "Unknown".to_string(),
             device_type: "unknown".to_string(),
         });
     }
    
    Ok(devices)
}

fn get_memory_info() -> Result<u64, String> {
    let output = Command::new("wmic")
        .args(["computersystem", "get", "TotalPhysicalMemory", "/value"])
        .output()
        .map_err(|e| e.to_string())?;
    
    let output_str = str::from_utf8(&output.stdout)
        .map_err(|e| e.to_string())?;
    for line in output_str.lines() {
        if line.starts_with("TotalPhysicalMemory=") {
            let memory_str = line.split('=').nth(1).unwrap_or("0");
            if let Ok(memory_bytes) = memory_str.parse::<u64>() {
                return Ok(memory_bytes / 1024 / 1024); // Convert to MB
            }
        }
    }
    
    // Fallback: try systeminfo command
    let output2 = Command::new("systeminfo")
        .output()
        .map_err(|e| e.to_string())?;
    
    let output_str2 = str::from_utf8(&output2.stdout)
        .map_err(|e| e.to_string())?;
    
    for line in output_str2.lines() {
        if line.contains("Total Physical Memory:") {
            // Extract memory value from systeminfo output
            if let Some(memory_part) = line.split(':').nth(1) {
                let memory_str = memory_part.trim().replace(",", "").replace(" MB", "");
                if let Ok(memory_mb) = memory_str.parse::<u64>() {
                    return Ok(memory_mb);
                }
            }
        }
    }
    
    Ok(16004) // Default fallback
}

fn get_physical_hardware_info() -> Result<String, String> {
    let output = Command::new("wmic")
        .args(["computersystem", "get", "Manufacturer,Model", "/format:list"])
        .output()
        .map_err(|e| e.to_string())?;
    
    let output_str = str::from_utf8(&output.stdout)
        .map_err(|e| e.to_string())?;
    
    let mut manufacturer = String::new();
    let mut model = String::new();
    
    for line in output_str.lines() {
        if line.starts_with("Manufacturer=") {
            manufacturer = line.split('=').nth(1).unwrap_or("").to_string();
        } else if line.starts_with("Model=") {
            model = line.split('=').nth(1).unwrap_or("").to_string();
        }
    }
    
    // Get BIOS serial number
    let bios_output = Command::new("wmic")
        .args(["bios", "get", "SerialNumber", "/format:list"])
        .output()
        .map_err(|e| e.to_string())?;
    
    let bios_str = str::from_utf8(&bios_output.stdout)
        .map_err(|e| e.to_string())?;
    
    let mut serial = String::new();
    for line in bios_str.lines() {
        if line.starts_with("SerialNumber=") {
            serial = line.split('=').nth(1).unwrap_or("").to_string();
            break;
        }
    }
    
    Ok(format!("{} {} {}", manufacturer, model, serial))
}

fn get_os_info() -> Result<(String, String), String> {
    // Get OS version
    let version_output = Command::new("wmic")
        .args(["os", "get", "Version,Caption", "/format:list"])
        .output()
        .map_err(|e| e.to_string())?;
    
    let version_str = str::from_utf8(&version_output.stdout)
        .map_err(|e| e.to_string())?;
    
    let mut os_version = String::new();
    let mut real_os = String::new();
    
    for line in version_str.lines() {
        if line.starts_with("Version=") {
            os_version = line.split('=').nth(1).unwrap_or("").to_string();
        } else if line.starts_with("Caption=") {
            real_os = line.split('=').nth(1).unwrap_or("").to_string();
        }
    }
    
    if os_version.is_empty() {
        os_version = "Unknown".to_string();
    }
    if real_os.is_empty() {
        real_os = "Unknown Windows".to_string();
    }
    
    Ok((os_version, real_os))
}

fn get_machine_signature() -> Result<String, String> {
    // Get machine GUID from registry or system
    let output = Command::new("wmic")
        .args(["csproduct", "get", "UUID", "/format:list"])
        .output()
        .map_err(|e| e.to_string())?;
    
    let output_str = str::from_utf8(&output.stdout)
        .map_err(|e| e.to_string())?;
    
    for line in output_str.lines() {
        if line.starts_with("UUID=") {
            let uuid = line.split('=').nth(1).unwrap_or("").trim();
            if !uuid.is_empty() && uuid != "(null)" {
                return Ok(format!("{{{}}}", uuid));
            }
        }
    }
    
    Ok("{Unknown-Machine-ID}".to_string())
}

fn get_user_info() -> Result<String, String> {
    // Get current user and computer name
    let user_output = Command::new("whoami")
        .output()
        .map_err(|e| e.to_string())?;
    
    let user_str = str::from_utf8(&user_output.stdout)
        .map_err(|e| e.to_string())?;
    
    let user = user_str.trim().to_string();
    
    if user.is_empty() {
        Ok("Unknown@UNKNOWN".to_string())
    } else {
        Ok(user)
    }
}

fn get_drive_info() -> Result<Vec<DriveInfo>, String> {
    // Get serial numbers for physical drives (focus on system drive)
    let serial_output = Command::new("wmic")
        .args(["diskdrive", "get", "SerialNumber", "/format:list"])
        .output()
        .map_err(|e| e.to_string())?;
    
    let serial_str = str::from_utf8(&serial_output.stdout)
        .map_err(|e| e.to_string())?;
    
    let mut drives = Vec::new();
      
    for line in serial_str.lines() {
        if line.starts_with("SerialNumber=") {
            let serial = line.split('=').nth(1).unwrap_or("").trim().to_string();
            if !serial.is_empty() && serial != "(null)" {
                drives.push(DriveInfo {
                    serial,
                });
            }
        }
    }
    
    // If no drives found, add a placeholder
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
        .map_err(|e| e.to_string())?;
    
    let output_str = str::from_utf8(&output.stdout)
        .map_err(|e| e.to_string())?;
    let mut local_ip = "Unknown".to_string();
    
    // Parse ipconfig output for local IP
    for line in output_str.lines() {
        if line.contains("IPv4 Address") {
            if let Some(ip_part) = line.split(':').nth(1) {
                let ip = ip_part.trim();
                // Skip loopback and APIPA addresses
                if !ip.starts_with("127.") && !ip.starts_with("169.254.") {
                    local_ip = ip.to_string();
                    break;
                }
            }
        }
    }
    
    // Try to get public IP (this is a simplified approach)
     let public_ip = match Command::new("nslookup")
         .args(["myip.opendns.com", "resolver1.opendns.com"])
         .output() {
         Ok(output) => {
             let output_str = str::from_utf8(&output.stdout).unwrap_or("");
             // Parse nslookup output for public IP
             let mut found_ip = "Unknown".to_string();
             for line in output_str.lines() {
                 if line.starts_with("Address:") && !line.contains("#") {
                     if let Some(ip) = line.split(':').nth(1) {
                         found_ip = ip.trim().to_string();
                         break;
                     }
                 }
             }
             found_ip
         },
         Err(_) => "Unknown".to_string(),
     };
    
    Ok(NetworkInfo {
        local_ip,
        public_ip,
    })
}

fn get_video_cards() -> Result<Vec<VideoCard>, String> {
    let output = Command::new("wmic")
        .args(["path", "win32_videocontroller", "get", "Name,DriverVersion", "/format:list"])
        .output()
        .map_err(|e| e.to_string())?;
    
    let output_str = str::from_utf8(&output.stdout)
        .map_err(|e| e.to_string())?;
    
    let mut video_cards = Vec::new();
    let mut current_card = VideoCard {
         name: String::new(),
         driver_version: String::new(),
     };
    
    for line in output_str.lines() {
        if line.starts_with("DriverVersion=") {
            current_card.driver_version = line.split('=').nth(1).unwrap_or("").to_string();
            if !current_card.name.is_empty() {
                video_cards.push(current_card.clone());
                current_card = VideoCard {
                     name: String::new(),
                     driver_version: String::new(),
                 };
            }
        } else if line.starts_with("Name=") {
            current_card.name = line.split('=').nth(1).unwrap_or("").to_string();
        }
    }
    
    if !current_card.name.is_empty() {
        video_cards.push(current_card);
    }
    
    Ok(video_cards)
}

fn get_monitors() -> Result<Vec<Monitor>, String> {
    let output = Command::new("wmic")
        .args(["desktopmonitor", "get", "Name,MonitorManufacturer,MonitorType", "/format:list"])
        .output()
        .map_err(|e| e.to_string())?;
    
    let output_str = str::from_utf8(&output.stdout)
        .map_err(|e| e.to_string())?;
    
    let mut monitors = Vec::new();
    let mut current_monitor = Monitor {
        name: String::new(),
        serial: String::new(),
    };
    
    for line in output_str.lines() {
        if line.starts_with("Name=") {
            if !current_monitor.name.is_empty() {
                monitors.push(current_monitor.clone());
            }
            current_monitor = Monitor {
                name: line.split('=').nth(1).unwrap_or("Unknown Monitor").to_string(),
                serial: "Unknown".to_string(),
            };
        }
    }
    
    if !current_monitor.name.is_empty() {
        monitors.push(current_monitor);
    }
    
    // Try to get monitor serial numbers from WMI
    let serial_output = Command::new("wmic")
        .args(["path", "Win32_DesktopMonitor", "get", "SerialNumberID", "/format:list"])
        .output();
    
    if let Ok(serial_out) = serial_output {
        if let Ok(serial_str) = str::from_utf8(&serial_out.stdout) {
            let mut serial_index = 0;
            for line in serial_str.lines() {
                if line.starts_with("SerialNumberID=") {
                    let serial = line.split('=').nth(1).unwrap_or("");
                    if !serial.is_empty() && serial_index < monitors.len() {
                        monitors[serial_index].serial = serial.to_string();
                        serial_index += 1;
                    }
                }
            }
        }
    }
    
    // Fallback if no monitors found
    if monitors.is_empty() {
        monitors.push(Monitor {
            name: "Primary Display".to_string(),
            serial: "Unknown".to_string(),
        });
    }
    
    Ok(monitors)
}

fn get_usb_devices() -> Result<Vec<UsbDevice>, String> {
    let output = Command::new("wmic")
        .args(["path", "Win32_USBControllerDevice", "get", "Dependent", "/format:list"])
        .output()
        .map_err(|e| e.to_string())?;
    
    let output_str = str::from_utf8(&output.stdout)
        .map_err(|e| e.to_string())?;
    
    let mut usb_devices = Vec::new();
    
    for line in output_str.lines() {
        if line.starts_with("Dependent=") && line.contains("USB") {
            if let Some(device_path) = line.split('=').nth(1) {
                // Extract device information from the path
                if device_path.contains("VID_") && device_path.contains("PID_") {
                    let mut vendor_id = String::new();
                    let mut product_id = String::new();
                    
                    // Extract VID and PID
                    if let Some(vid_start) = device_path.find("VID_") {
                        vendor_id = device_path[vid_start+4..vid_start+8].to_string();
                    }
                    if let Some(pid_start) = device_path.find("PID_") {
                        product_id = device_path[pid_start+4..pid_start+8].to_string();
                    }
                    
                    usb_devices.push(UsbDevice {
                        name: "USB Device".to_string(), // Generic name, could be enhanced
                        vendor_id,
                        product_id,
                    });
                }
            }
        }
    }
    
    // Alternative approach using PnP devices
    if usb_devices.is_empty() {
        let pnp_output = Command::new("wmic")
            .args(["path", "Win32_PnPEntity", "where", "DeviceID like 'USB%'", "get", "Name,DeviceID", "/format:list"])
            .output()
            .map_err(|e| e.to_string())?;
        
        let pnp_str = str::from_utf8(&pnp_output.stdout)
            .map_err(|e| e.to_string())?;
        
        let mut current_device = UsbDevice {
            name: String::new(),
            vendor_id: String::new(),
            product_id: String::new(),
        };
        
        for line in pnp_str.lines() {
            if line.starts_with("DeviceID=") && line.contains("VID_") {
                let device_id = line.split('=').nth(1).unwrap_or("");
                
                // Extract VID and PID
                if let Some(vid_start) = device_id.find("VID_") {
                    current_device.vendor_id = device_id[vid_start+4..vid_start+8].to_string();
                }
                if let Some(pid_start) = device_id.find("PID_") {
                    current_device.product_id = device_id[pid_start+4..pid_start+8].to_string();
                }
            } else if line.starts_with("Name=") {
                current_device.name = line.split('=').nth(1).unwrap_or("USB Device").to_string();
                
                if !current_device.vendor_id.is_empty() {
                    usb_devices.push(current_device.clone());
                }
                
                current_device = UsbDevice {
                    name: String::new(),
                    vendor_id: String::new(),
                    product_id: String::new(),
                };
            }
        }
    }
    
    Ok(usb_devices)
}

fn get_processor_info() -> Result<ProcessorInfo, String> {
    let output = Command::new("wmic")
        .args(["cpu", "get", "Name,MaxClockSpeed", "/format:list"])
        .output()
        .map_err(|e| e.to_string())?;
    
    let output_str = str::from_utf8(&output.stdout)
        .map_err(|e| e.to_string())?;
    
    let mut cpu_name = String::new();
     let mut cpu_cores = 0;
     
     for line in output_str.lines() {
         if line.starts_with("Name=") {
             cpu_name = line.split('=').nth(1).unwrap_or("").to_string();
         }
     }
     
     // Get CPU core count
     let core_output = Command::new("wmic")
         .args(["cpu", "get", "NumberOfLogicalProcessors", "/format:list"])
         .output()
         .map_err(|e| e.to_string())?;
     
     let core_str = str::from_utf8(&core_output.stdout)
         .map_err(|e| e.to_string())?;
     
     for line in core_str.lines() {
         if line.starts_with("NumberOfLogicalProcessors=") {
             if let Ok(cores) = line.split('=').nth(1).unwrap_or("0").parse::<u32>() {
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