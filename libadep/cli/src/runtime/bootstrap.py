#!/usr/bin/env python3
import os
import subprocess
import sys
from pathlib import Path

APP_ROOT = Path(os.environ["ADEP_APP_ROOT"])
ENTRY = APP_ROOT / os.environ["ADEP_ENTRY"]
CACHE_DIR = os.environ.get("PIP_CACHE_DIR", "/pip-cache")
REQ_PATH = os.environ.get("ADEP_DEP_PATH")
WHEELS_DIR = os.environ.get("ADEP_WHEELS")

def install_requirements():
    if not REQ_PATH:
        return
    req_file = APP_ROOT / REQ_PATH
    # Check if requirements file contains hashes
    has_hashes = False
    if req_file.exists():
        content = req_file.read_text()
        has_hashes = "--hash=" in content
    
    cmd = [
        sys.executable,
        "-m",
        "pip",
        "install",
    ]
    if has_hashes:
        cmd.append("--require-hashes")
    cmd.extend([
        "--cache-dir",
        CACHE_DIR,
        "--no-input",
        "--no-color",
        "--disable-pip-version-check",
    ])
    if WHEELS_DIR:
        cmd.extend(["--find-links", (APP_ROOT / WHEELS_DIR).as_posix()])
        cmd.append("--prefer-binary")
    cmd.extend(["-r", req_file.as_posix()])
    subprocess.check_call(cmd)

if __name__ == "__main__":
    install_requirements()
    os.execv(sys.executable, [sys.executable, ENTRY.as_posix()])

