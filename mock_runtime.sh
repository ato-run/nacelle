#!/bin/sh
# Mock runtime that accepts any arguments and exits successfully
# It writes the PID to the pid-file specified by --pid-file arg

echo "Mock runtime called with args: $@" >&2

# Check if command is "state"
if [ "$1" = "state" ]; then
  # Output mock state JSON
  echo "{\"pid\": $$}"
  exit 0
fi

# Parse arguments to find --pid-file
PID_FILE=""
while [[ $# -gt 0 ]]; do
  case $1 in
    --pid-file)
      PID_FILE="$2"
      shift # past argument
      shift # past value
      ;;
    *)
      shift # past argument
      ;;
  esac
done

if [ -n "$PID_FILE" ]; then
  echo $$ > "$PID_FILE"
fi

# Simulate running process
sleep 1
