#!/usr/bin/env python3

import subprocess
import threading
import time
import json
import uuid
import sys
import os
from queue import Queue, Empty

# --- Configuration ---
# Use the command exactly as seen in the mcp_host logs
SUPABASE_COMMAND = [
    "npx",
    "-y",
    "@supabase/mcp-server-supabase@latest",
    "--access-token",
    "sbp_6dd1b03bb0c829ebf4b2607a3a5e114ff607e83f", # Replace with your actual token if different/expired
]
# Set environment variables if needed (e.g., if the server requires them)
SERVER_ENV = os.environ.copy()
# SERVER_ENV["SOME_VAR"] = "some_value" # Example

# Timeouts (seconds)
STARTUP_WAIT = 5  # Time to wait for server to potentially start up
READ_INTERVAL = 0.1 # How often to check for output
POST_TOOLS_LIST_WAIT = 10 # How long to wait for output after sending tools/list

# --- Helper Functions ---

def make_request(method, params, req_id=None):
    """Creates a JSON-RPC request string."""
    if req_id is None:
        req_id = str(uuid.uuid4())
    req = {
        "jsonrpc": "2.0",
        "method": method,
        "id": req_id,
    }
    if params is not None:
        req["params"] = params
    return json.dumps(req) + "\n" # Ensure newline termination

def make_notification(method, params):
    """Creates a JSON-RPC notification string."""
    notif = {
        "jsonrpc": "2.0",
        "method": method,
    }
    if params is not None:
        notif["params"] = params
    return json.dumps(notif) + "\n" # Ensure newline termination

def stream_reader(stream, queue, stream_name):
    """Reads a stream byte by byte and puts lines/chunks onto a queue."""
    try:
        for chunk in iter(lambda: stream.read(1), b''):
            queue.put((stream_name, chunk))
    except ValueError:
        # Handle case where stream is closed prematurely
        print(f"[{stream_name}] Stream closed unexpectedly.", file=sys.stderr)
    finally:
        queue.put((stream_name, None)) # Signal EOF

def print_output(queue):
    """Prints output from the queue, handling bytes."""
    streams_open = {'stdout': True, 'stderr': True}
    buffer = {'stdout': b'', 'stderr': b''}

    while streams_open['stdout'] or streams_open['stderr']:
        try:
            stream_name, chunk = queue.get(timeout=READ_INTERVAL)
            if chunk is None:
                # EOF for this stream
                if buffer[stream_name]:
                    try:
                        print(f"[{stream_name}-PARTIAL-EOF] {buffer[stream_name].decode('utf-8', errors='replace')}", flush=True)
                    except Exception as e:
                         print(f"[{stream_name}-PARTIAL-EOF-ERROR] Error decoding: {e}. Raw: {buffer[stream_name]!r}", flush=True)
                print(f"[{stream_name}] <EOF>", flush=True)
                streams_open[stream_name] = False
                buffer[stream_name] = b'' # Clear buffer on EOF
                continue

            buffer[stream_name] += chunk

            # Try to decode and print lines
            while True:
                try:
                    line, separator, rest = buffer[stream_name].partition(b'\n')
                    if separator:
                        # Found a newline
                        try:
                            print(f"[{stream_name}] {line.decode('utf-8', errors='replace')}", flush=True)
                        except Exception as e:
                            print(f"[{stream_name}-ERROR] Error decoding: {e}. Raw: {line!r}", flush=True)
                        buffer[stream_name] = rest # Keep the remainder
                    else:
                        # No newline found, keep buffering
                        break
                except Exception as e:
                    print(f"[{stream_name}-FATAL] Error processing buffer: {e}. Buffer: {buffer[stream_name]!r}", flush=True)
                    buffer[stream_name] = b'' # Clear buffer on error to prevent loops
                    break

        except Empty:
            # Queue is empty, check if process is still running
            pass
        except Exception as e:
            print(f"[PrinterThread-ERROR] {e}", file=sys.stderr)
            break # Exit printer thread on unexpected error

    print("[PrinterThread] Exiting.", file=sys.stderr)


# --- Main Execution ---
if __name__ == "__main__":
    print(f"Starting server process: {' '.join(SUPABASE_COMMAND)}")
    process = None
    try:
        process = subprocess.Popen(
            SUPABASE_COMMAND,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=SERVER_ENV,
            bufsize=0 # Unbuffered
        )
        print(f"Server process started with PID: {process.pid}")

        output_queue = Queue()

        # Start reader threads
        stdout_thread = threading.Thread(target=stream_reader, args=(process.stdout, output_queue, 'stdout'), daemon=True)
        stderr_thread = threading.Thread(target=stream_reader, args=(process.stderr, output_queue, 'stderr'), daemon=True)
        printer_thread = threading.Thread(target=print_output, args=(output_queue,), daemon=True)

        stdout_thread.start()
        stderr_thread.start()
        printer_thread.start()

        print(f"\nWaiting {STARTUP_WAIT}s for server startup...")
        time.sleep(STARTUP_WAIT)

        # 1. Send initialize request
        print("\n>>> Sending initialize request...")
        init_req_id = "debug-init-" + str(uuid.uuid4())
        init_params = {
            "protocolVersion": "2025-03-26",
            "capabilities": {"experimental": {}, "sampling": {}, "roots": {"list_changed": False}},
            "clientInfo": {"name": "debug-script-client", "version": "1.0.0"}
        }
        init_req = make_request("initialize", init_params, init_req_id)
        print(f">>> {init_req.strip()}")
        process.stdin.write(init_req.encode('utf-8'))
        process.stdin.flush()
        print(">>> Initialize request sent. Waiting briefly for response...")
        time.sleep(2) # Give time for initialize response to be read

        # 2. Send initialized notification
        print("\n>>> Sending initialized notification...")
        initialized_notif = make_notification("notifications/initialized", None)
        print(f">>> {initialized_notif.strip()}")
        process.stdin.write(initialized_notif.encode('utf-8'))
        process.stdin.flush()
        print(">>> Initialized notification sent.")
        time.sleep(1)

        # 3. Send tools/list request
        print("\n>>> Sending tools/list request...")
        tools_req_id = "debug-tools-" + str(uuid.uuid4())
        tools_req = make_request("tools/list", None, tools_req_id)
        print(f">>> {tools_req.strip()}")
        process.stdin.write(tools_req.encode('utf-8'))
        process.stdin.flush()
        print(f">>> Tools/list request sent. Monitoring output for {POST_TOOLS_LIST_WAIT}s...")

        # Wait and let reader threads capture output
        time.sleep(POST_TOOLS_LIST_WAIT)

        print(f"\n--- Finished waiting after tools/list ---")

    except FileNotFoundError:
        print(f"Error: Command 'npx' not found. Is Node.js/npm installed and in your PATH?", file=sys.stderr)
        sys.exit(1)
    except BrokenPipeError:
         print(f"Error: Broken pipe. The server process likely terminated unexpectedly.", file=sys.stderr)
    except Exception as e:
        print(f"\n--- An error occurred ---")
        print(f"Error: {e}", file=sys.stderr)
    finally:
        if process:
            print("\n--- Terminating server process ---")
            process.terminate() # Send SIGTERM
            try:
                process.wait(timeout=5) # Wait for termination
                print(f"Server process terminated with code: {process.returncode}")
            except subprocess.TimeoutExpired:
                print("Server process did not terminate gracefully, killing.")
                process.kill() # Send SIGKILL
                process.wait()
                print(f"Server process killed.")
            except Exception as e:
                 print(f"Error during process cleanup: {e}", file=sys.stderr)

            # Ensure reader threads are signaled to stop (closing streams should do this)
            if process.stdin:
                try: process.stdin.close()
                except: pass
            if process.stdout:
                try: process.stdout.close()
                except: pass
            if process.stderr:
                try: process.stderr.close()
                except: pass

        # Wait for printer thread to finish processing queue
        print("Waiting for printer thread to finish...")
        if 'printer_thread' in locals() and printer_thread.is_alive():
             # Give it a bit more time to flush the queue
             time.sleep(2)
             # The printer thread should exit once both stream readers send None
             # but we join defensively
             printer_thread.join(timeout=5)
             if printer_thread.is_alive():
                 print("Printer thread did not exit cleanly.", file=sys.stderr)

        print("\n--- Script finished ---")
