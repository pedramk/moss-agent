# MOSS Agent - Rust Backend

This is the Rust backend component of the MOSS system monitoring application. It provides a gRPC server that captures system events and hardware information.

## Building

### Standard Build
```bash
cargo build --release
```

### Build and Copy to Electron App
To build the agent and automatically copy it to the Electron app's bin folder, use one of these scripts:

**Windows Batch (Recommended):**
```cmd
.\build-and-copy.bat
```

**PowerShell:**
```powershell
.\build-and-copy.ps1
```

These scripts will:
1. Build the Rust agent in release mode
2. Create the `../app/bin/` directory if it doesn't exist
3. Copy the compiled `agent.exe` to `../app/bin/agent.exe`
4. Provide feedback on the build and copy process

## Features

- **Input Event Monitoring**: Captures keyboard and mouse events using the `rdev` library
- **System Information Collection**: Gathers comprehensive hardware and system details
- **Real-time Change Detection**: Monitors for system changes every 5 seconds
- **gRPC Server**: Provides streaming API on `localhost:50051`
- **Smart Event Filtering**: Prevents duplicate events and throttles mouse movements

## Dependencies

- `tokio` - Async runtime
- `tonic` - gRPC framework
- `rdev` - Cross-platform input event capture
- `serde` - Serialization framework
- `chrono` - Date and time handling

## Protocol Buffers

The gRPC interface is defined in `proto/capture.proto` and is automatically compiled during the build process.