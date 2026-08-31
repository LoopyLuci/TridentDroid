# Building TridentDroid

## Prerequisites

### Rust
- Rust 1.70+ with `stable-x86_64-pc-windows-msvc` toolchain
- Cargo

### Node.js
- Node.js 18+ with npm

### WiX Toolset (for MSI installer)
1. Download WiX Toolset v3.x from https://wixtoolset.org/
2. Install it (default path: `C:\Program Files (x86)\WiX Toolset v3.11\`)
3. Add to PATH: `C:\Program Files (x86)\WiX Toolset v3.11\bin`

## Development

```bash
cd tauri-gui
npm install
cargo tauri dev
```

## Building

### Debug Build
```bash
cd tauri-gui
cargo tauri build
```

### Release Build
```bash
cd tauri-gui
cargo tauri build --release
```

### MSI Installer
```bash
cd tauri-gui
cargo tauri bundle --target msi
```

The MSI will be at:
```
tauri-gui/src-tauri/target/release/bundle/msi/TridentDroid_0.1.0_x64_en-US.msi
```

## Project Structure

```
tauri-gui/
├── src/                    # React + TypeScript frontend
│   ├── components/         # Reusable UI components
│   ├── pages/              # Route pages
│   ├── hooks/              # React hooks
│   ├── lib/                # Utilities
│   └── styles/             # CSS
├── src-tauri/              # Rust Tauri backend
│   ├── src/
│   │   ├── main.rs         # Entry point
│   │   ├── commands.rs     # IPC commands
│   │   ├── client.rs       # gRPC client
│   │   └── streaming.rs    # Stream management
│   ├── icons/              # App icons
│   └── Cargo.toml
└── package.json
```

## Architecture

The GUI connects to the tridentd daemon via gRPC:

```
[Tauri GUI] ←→ [gRPC Client] ←→ [tridentd Daemon] ←→ [KVM/WHP]
```

### Tauri Commands (IPC)
- `ping_daemon` — Health check
- `launch_instance` — Create and start a VM
- `stop_instance` — Stop a VM
- `list_instances` — List all VMs
- `get_instance_info` — Get VM details
- `fork_instance` — COW fork a VM
- `start_adb_shell` — Open ADB shell session
- `send_adb_command` — Send command to ADB shell
- `start_display_stream` — Start display streaming

### Streaming
ADB shell and display use bidirectional gRPC streams with Tauri events:
- `adb_shell_{id}` — Shell output events
- `display_frame_{id}` — Display frame events

### mTLS
The GUI supports mutual TLS for daemon connection:
1. Enable TLS in Settings
2. Provide CA certificate path
3. Provide client certificate and key paths
4. Connection will use HTTPS with client cert verification
