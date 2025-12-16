use anyhow::Result;
use clap::Args;
use dirs::home_dir;
use std::process::Command;

#[derive(Args)]
pub struct DoctorArgs {}

pub fn run(_args: &DoctorArgs) -> Result<()> {
    println!("🏥 ADEP Environment Check");
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Check container engines
    println!("Container Engines:");
    let has_podman = check_podman();
    let has_docker = check_docker();

    if has_podman {
        println!("  ✅ podman    (preferred)");
    } else {
        println!("  ❌ podman    (not found)");
    }

    if has_docker {
        if has_podman {
            println!("  ✅ docker    (available)");
        } else {
            println!("  ✅ docker    (fallback)");
        }
    } else {
        println!("  ❌ docker    (not found)");
    }

    println!();

    // Check runtime tools
    println!("Runtime Tools:");
    check_python();

    println!();

    // Check cache directory
    println!("Cache Directory:");
    check_cache_dir();

    println!();

    // Check port availability
    println!("Port Availability:");
    check_ports();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    // Summary
    if (has_podman || has_docker) && check_python_exists() {
        println!("✅ Your system is ready to run ADEP containers!");
    } else {
        println!("⚠️  Some dependencies are missing. See installation instructions below:");
        println!();

        if !has_podman && !has_docker {
            print_engine_install_instructions();
        }

        if !check_python_exists() {
            print_python_install_instructions();
        }
    }

    Ok(())
}

fn check_podman() -> bool {
    Command::new("podman")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn check_docker() -> bool {
    Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn check_python_exists() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn check_python() {
    if let Ok(output) = Command::new("python3").arg("--version").output() {
        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            let version = version.trim().replace("Python ", "");
            println!("  ✅ python3   ({})", version);
            return;
        }
    }
    println!("  ❌ python3   (not found)");
}

fn check_cache_dir() {
    if let Some(home) = home_dir() {
        let adep_dir = home.join(".adep");
        if adep_dir.exists() {
            println!("  ✅ ~/.adep/  (exists)");
        } else {
            println!("  ℹ️  ~/.adep/  (will be created on first run)");
        }
    } else {
        println!("  ⚠️  Cannot resolve HOME directory");
    }
}

fn check_ports() {
    use std::net::TcpListener;

    let port_range = 8000..=8010;
    let mut available_ports = Vec::new();

    for port in port_range.clone() {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            available_ports.push(port);
        }
    }

    let total = port_range.count();
    let available = available_ports.len();

    if available == 0 {
        println!("  ⚠️  Ports      All ports in range 8000-8010 are occupied!");
        println!("              Consider stopping some services or using --port");
        #[cfg(unix)]
        {
            println!();
            println!("              Check with: lsof -i :8000-8010");
        }
    } else if available < 3 {
        println!(
            "  ⚠️  Ports      {}/{} available (8000-8010)",
            available, total
        );
        println!("              Available: {:?}", available_ports);
    } else {
        println!(
            "  ✅ Ports      {}/{} available (8000-8010)",
            available, total
        );
    }
}

fn print_engine_install_instructions() {
    println!("📦 Install Container Engine:");
    println!();

    #[cfg(target_os = "macos")]
    {
        println!("  # Install podman (recommended):");
        println!("  brew install podman");
        println!("  podman machine init");
        println!("  podman machine start");
        println!();
        println!("  # Or install docker:");
        println!("  brew install --cask docker");
    }

    #[cfg(target_os = "linux")]
    {
        println!("  # Ubuntu/Debian:");
        println!("  sudo apt install podman");
        println!();
        println!("  # Fedora/RHEL:");
        println!("  sudo dnf install podman");
        println!();
        println!("  # Docker (alternative):");
        println!("  curl -fsSL https://get.docker.com | sh");
    }

    println!();
}

fn print_python_install_instructions() {
    println!("🐍 Install Python:");
    println!();

    #[cfg(target_os = "macos")]
    {
        println!("  brew install python@3.11");
    }

    #[cfg(target_os = "linux")]
    {
        println!("  # Ubuntu/Debian:");
        println!("  sudo apt install python3");
        println!();
        println!("  # Fedora/RHEL:");
        println!("  sudo dnf install python3");
    }

    println!();
}
