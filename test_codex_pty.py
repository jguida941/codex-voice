#!/usr/bin/env python3
"""Test Codex with PTY for proper terminal emulation."""

import os
import pty
import select
import subprocess
import time
import sys

def test_codex_with_pty():
    """Test Codex using PTY for full terminal emulation."""
    print("Testing Codex with PTY (pseudo-terminal)...")

    # Create a PTY
    master, slave = pty.openpty()

    # Start Codex with the slave PTY as its terminal
    cmd = ["codex", "--skip-git-repo-check"]

    try:
        proc = subprocess.Popen(
            cmd,
            stdin=slave,
            stdout=slave,
            stderr=slave,
            cwd="..",  # Parent directory
            preexec_fn=os.setsid  # Create new session
        )

        # Close slave in parent (child has it)
        os.close(slave)

        print(f"Codex started with PID: {proc.pid}")

        # Function to read available data
        def read_output(timeout=2):
            output = b""
            end_time = time.time() + timeout
            while time.time() < end_time:
                if select.select([master], [], [], 0.1)[0]:
                    try:
                        data = os.read(master, 1024)
                        if data:
                            output += data
                    except OSError:
                        break
                if proc.poll() is not None:
                    break
            return output.decode('utf-8', errors='ignore')

        # Read initial output
        initial = read_output(1)
        if initial:
            print(f"Initial output: {initial[:100]}...")

        # Test 1: Send first prompt
        print("\nTest 1: Sending 'Hello'...")
        os.write(master, b"Hello\n")

        response1 = read_output(3)
        if response1:
            print(f"Response 1: {response1[:200]}...")

        # Check if still alive
        if proc.poll() is None:
            print("✅ Process still alive after first prompt")

            # Test 2: Send second prompt
            print("\nTest 2: Sending 'What is 2+2?'...")
            os.write(master, b"What is 2+2?\n")

            response2 = read_output(3)
            if response2:
                print(f"Response 2: {response2[:200]}...")

            # Final check
            if proc.poll() is None:
                print("\n✅ SUCCESS: Codex stayed alive for multiple prompts!")
                print("We can use PTY for persistent sessions.")

                # Test 3: Context persistence
                print("\nTest 3: Testing context - 'My name is Alice'...")
                os.write(master, b"My name is Alice\n")
                read_output(2)

                print("Test 4: Asking 'What is my name?'...")
                os.write(master, b"What is my name?\n")
                response3 = read_output(3)
                if "Alice" in response3:
                    print("✅ Context maintained across prompts!")
                else:
                    print("⚠️ Context might not be maintained")

                # Clean shutdown
                os.write(master, b"exit\n")
                time.sleep(0.5)
            else:
                print("❌ Process died after second prompt")
        else:
            print("❌ Process died after first prompt")

        # Clean up
        proc.terminate()
        proc.wait(timeout=2)
        os.close(master)

    except FileNotFoundError:
        print("ERROR: codex not found")
        return False
    except Exception as e:
        print(f"ERROR: {e}")
        import traceback
        traceback.print_exc()
        return False

    return proc.poll() is None or proc.returncode == 0

if __name__ == "__main__":
    success = test_codex_with_pty()
    sys.exit(0 if success else 1)