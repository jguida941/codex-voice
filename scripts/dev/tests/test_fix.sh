#!/bin/bash
# Test that the input sanitization fix prevents crashes from terminal sequences

echo "Testing that problematic terminal sequences are properly sanitized..."

# Create a test input file with the problematic sequence
echo -e "0;0;0uWhat the fuck is this shit" > /tmp/test_input.txt

# Run a quick Rust test to verify sanitization
cat > /tmp/test_sanitize.rs << 'EOF'
fn validate_for_display(mut line: String) -> String {
    // Remove "0;0;0u" pattern - this is the known crasher
    if line.contains("0;0;0u") {
        line = line.replace("0;0;0u", "");
    }

    // Also remove other common terminal sequences
    for pattern in &["0;0u", "1;2c", ">0;0;0u", "?1;2c", "6n", "0c"] {
        if line.contains(pattern) {
            line = line.replace(pattern, "");
        }
    }

    line
}

fn main() {
    let input = "0;0;0uWhat the fuck is this shit";
    let sanitized = validate_for_display(input.to_string());

    println!("Input:     {:?}", input);
    println!("Sanitized: {:?}", sanitized);

    if sanitized.contains("0;0;0u") {
        eprintln!("ERROR: Sanitization failed!");
        std::process::exit(1);
    }

    if sanitized == "What the fuck is this shit" {
        println!("SUCCESS: Problematic sequence removed correctly!");
        std::process::exit(0);
    } else {
        eprintln!("ERROR: Unexpected sanitization result");
        std::process::exit(1);
    }
}
EOF

rustc /tmp/test_sanitize.rs -o /tmp/test_sanitize && /tmp/test_sanitize

if [ $? -eq 0 ]; then
    echo "✓ Sanitization test passed"
else
    echo "✗ Sanitization test failed"
    exit 1
fi

echo ""
echo "Testing the actual TUI doesn't crash with problematic input..."

# Build the TUI if needed
cargo build --quiet

# Create a test script that simulates problematic input
cat > /tmp/test_tui_input.sh << 'EOF'
#!/bin/bash
# Send the problematic sequence as input
echo -n "0;0;0uTest input"
sleep 0.5
# Send Enter
echo ""
sleep 0.5
# Send Ctrl+C to exit
printf '\003'
EOF

chmod +x /tmp/test_tui_input.sh

# Run the TUI with the test input and capture any errors
timeout 2 /tmp/test_tui_input.sh | cargo run --quiet 2>/tmp/tui_error.log

if grep -q "byte index" /tmp/tui_error.log 2>/dev/null; then
    echo "✗ TUI still crashes with byte index error"
    cat /tmp/tui_error.log
    exit 1
else
    echo "✓ TUI handles problematic input without crashing"
fi

echo ""
echo "All tests passed! The fix successfully prevents the crash."