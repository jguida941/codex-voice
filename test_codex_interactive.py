#!/usr/bin/env python3
"""Test if we can keep Codex alive in interactive mode."""

import subprocess
import time
import sys
import select
import os

def test_codex_interactive():
    """Test Codex in interactive mode with multiple prompts."""
    print("Testing Codex interactive mode...")

    # Start Codex in interactive mode (no 'exec' subcommand)
    # Using the parent directory as working directory
    cmd = ["codex", "--skip-git-repo-check"]

    try:
        # Start process with pipes
        proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=0,  # Unbuffered
            cwd=".."  # Parent directory
        )

        print(f"Codex process started with PID: {proc.pid}")

        # Test 1: Send first prompt
        print("\nTest 1: Sending 'Hello'...")
        proc.stdin.write("Hello\n")
        proc.stdin.flush()

        # Read response with timeout
        time.sleep(2)

        # Check if there's output available
        if select.select([proc.stdout], [], [], 0.1)[0]:
            output = proc.stdout.read(1024)
            print(f"Response 1: {output[:100]}...")

        # Test 2: Send second prompt (testing persistence)
        print("\nTest 2: Sending 'What is 2+2?'...")
        proc.stdin.write("What is 2+2?\n")
        proc.stdin.flush()

        time.sleep(2)

        if select.select([proc.stdout], [], [], 0.1)[0]:
            output = proc.stdout.read(1024)
            print(f"Response 2: {output[:100]}...")

        # Check if process is still alive
        if proc.poll() is None:
            print("\n✅ SUCCESS: Codex process is still alive after multiple prompts!")
            print("This means we can implement persistent session.")
        else:
            print("\n❌ FAILED: Codex process exited.")
            print(f"Return code: {proc.returncode}")

        # Clean up
        proc.terminate()
        proc.wait(timeout=2)

    except FileNotFoundError:
        print("ERROR: codex command not found")
        return False
    except Exception as e:
        print(f"ERROR: {e}")
        return False

    return True

if __name__ == "__main__":
    success = test_codex_interactive()
    sys.exit(0 if success else 1)