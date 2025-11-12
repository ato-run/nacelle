# Rig-Manager

This is the control plane for the Rig, an OCI-compatible rig.

## Description

This project provides the API server and control plane for managing Rig. It is built with Rust using the Axum web framework and SQLx for database interaction.

## Requirements

- Rust (latest stable version recommended)
- SQLite

## Setup

1.  **Clone the repository:**

    ```bash
    git clone <repository-url>
    cd rig-manager
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

4.  **Setup the database:**
    This project uses SQLx for database migrations. You will need to have `sqlx-cli` installed.

    ```bash
    cargo install sqlx-cli
    ```

    Then, run the migrations:

    ```bash
    sqlx database create
    sqlx migrate run
    ```

5.  **Generate an API key:**
    The control plane uses an API key for authentication. The key is stored in `api_key.txt`.
    Install the provisioning script dependencies if you haven't already, then generate a new key:
    ```bash
    pip3 install -r provisioning/scripts/requirements.txt
    python3 provisioning/scripts/manage_api_keys.py generate
    ```
    This will create the `api_key.txt` file. This file is ignored by git.

## Running the application

Once the setup is complete, you can run the control plane with:

```bash
cargo run
```

The server will start on the address specified in your `config.toml` file.
