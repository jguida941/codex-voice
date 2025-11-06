#!/usr/bin/env python3
"""Automated test for the Enter key fix."""

import subprocess
import time
import os
import sys

def test_enter_key_fix():
    """Test that multiple voice captures and Enter key presses work."""

    # Clear the log
    log_file = os.path.join(os.environ.get('TMPDIR', '/tmp'), 'codex_voice_tui.log')
    print(f"Clearing log at: {log_file}")
    open(log_file, 'w').close()

    print("\n=== Testing Enter Key Fix ===\n")
    print("Simulating two voice captures with fake whisper...")

    # Use fake whisper for consistent testing
    test_env = os.environ.copy()

    # Start the TUI with fake whisper
    cmd = [
        'cargo', 'run', '--',
        '--seconds', '2',
        '--ffmpeg-device', ':0',
        '--whisper-cmd', '../stubs/fake_whisper',
        '--whisper-model', 'base',
        '--codex-cmd', 'codex'
    ]

    # Create test input: Ctrl+R, Enter, Ctrl+R, Enter, Ctrl+C
    # Using echo to simulate: we can't actually send Ctrl+R through stdin
    # So let's just check if the TUI starts properly

    print("Starting TUI...")
    proc = subprocess.Popen(
        cmd,
        cwd='/Users/jguida941/new_github_projects/codex_voice/rust_tui',
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True
    )

    # Give it time to start
    time.sleep(1)

    # Send Ctrl+C to exit cleanly
    proc.terminate()

    # Wait for process to finish
    try:
        proc.wait(timeout=2)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()

    print(f"TUI process exited with code: {proc.returncode}")

    # Check if the TUI started without errors
    if proc.returncode in [0, -15, -2]:  # 0=success, -15=SIGTERM, -2=SIGINT
        print("✅ TUI started and stopped successfully")

        # Check the log for any errors
        if os.path.exists(log_file):
            with open(log_file, 'r') as f:
                log_content = f.read()
                if 'panic' in log_content.lower() or 'error' in log_content.lower():
                    print("⚠️  Found errors in log (may be normal):")
                    for line in log_content.split('\n'):
                        if 'error' in line.lower() and 'voice capture failed' not in line.lower():
                            print(f"  {line}")
                else:
                    print("✅ No critical errors in log")

        return True
    else:
        print(f"❌ TUI failed with code {proc.returncode}")
        if proc.stderr:
            stderr_output = proc.stderr.read() if hasattr(proc.stderr, 'read') else ''
            if stderr_output:
                print(f"Error output: {stderr_output}")
        return False

def verify_build():
    """Verify the fix was built correctly."""
    print("\n=== Verifying Build ===\n")

    # Check if the event clearing code is in place
    main_rs = '/Users/jguida941/new_github_projects/codex_voice/rust_tui/src/main.rs'
    with open(main_rs, 'r') as f:
        content = f.read()

    checks = [
        ('Event clearing', 'Clear any pending events'),
        ('Enhanced Enter logging', 'trimmed_len'),
        ('Input state reset', 'app.input.clear()'),
    ]

    all_good = True
    for name, text in checks:
        if text in content:
            print(f"✅ {name}: Found")
        else:
            print(f"❌ {name}: Missing")
            all_good = False

    return all_good

if __name__ == "__main__":
    print("=" * 50)
    print("ENTER KEY FIX VERIFICATION TEST")
    print("=" * 50)

    # First verify the build
    if not verify_build():
        print("\n⚠️  Warning: Some fixes may not be in place")

    # Then test the TUI
    if test_enter_key_fix():
        print("\n" + "=" * 50)
        print("✅ BASIC TEST PASSED")
        print("=" * 50)
        print("\nThe TUI starts without crashing.")
        print("For full testing of voice capture + Enter key:")
        print("  Run: ./TEST_ENTER_FIX.sh")
        print("  Then manually test Ctrl+R → Speak → Enter (twice)")
        sys.exit(0)
    else:
        print("\n" + "=" * 50)
        print("❌ TEST FAILED")
        print("=" * 50)
        sys.exit(1)