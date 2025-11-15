# Capsuled Engine

This is the execution agent for Capsuled, responsible for running capsules (containers) on individual nodes.

## Description

Capsuled Engine is the agent component that runs on each node in the Capsuled cluster. It receives instructions from the Capsuled Coordinator (Client) via gRPC and manages the lifecycle of capsules using OCI-compatible runtimes. It is built with Rust and provides features like GPU detection, container management, and hardware monitoring.

## Requirements

- Rust (latest stable version recommended)
- youki or compatible OCI runtime
- protobuf compiler (protoc)
- Optional: NVIDIA GPU and drivers for GPU workload support

## Setup

1.  **Clone the repository:**

    ```bash
    git clone https://github.com/OnesCluster/capsuled.git
    cd capsuled/engine
    ```

2.  **Create the environment file:**
    Copy the example environment file and configure it as needed.

    ```bash
    cp .env.example .env
    ```

3.  **Create the configuration file:**
    Copy the example configuration file.

    ```bash
    cp config.toml.example config.toml
    ```

4.  **Build the adep-logic Wasm module:**
    The engine requires the adep-logic Wasm module for manifest validation.

    ```bash
    cd ../adep-logic
    cargo build --release --target wasm32-unknown-unknown
    cd ../engine
    ```

5.  **Optional: Setup provisioning scripts**
    If you're deploying to a production environment, you can use the provisioning scripts:
    ```bash
    pip3 install -r provisioning/scripts/requirements.txt
    python3 provisioning/scripts/deploy.py
    ```

## Running the application

Once the setup is complete, you can run the engine with:

```bash
# For development
cargo run

# With custom configuration
cargo run -- --addr 0.0.0.0:50051 --coordinator-addr http://coordinator:50052

# For production (build release binary first)
cargo build --release
./target/release/capsuled-engine
```

The gRPC server will start on the address specified in your configuration (default: 0.0.0.0:50051).

## Architecture

Capsuled Engine is part of the larger Capsuled distributed system:

- **Capsuled Coordinator (Client)**: Cluster management, scheduling, and master election
- **Capsuled Engine (this component)**: Container execution and hardware management
- **adep-logic**: Shared validation logic compiled to WebAssembly

For more details, see the [main ARCHITECTURE.md](../ARCHITECTURE.md) in the repository root.
