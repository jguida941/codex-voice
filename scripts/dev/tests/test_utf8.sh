#!/bin/bash

# Test that multi-byte UTF-8 characters are preserved correctly

echo "Testing UTF-8 character preservation..."
echo ""

# Display a test input with various UTF-8 characters
cat << 'EOF'
Hello café
Testing émoji: 🎉
Chinese: 你好
Japanese: こんにちは
Arabic: مرحبا
Math: ∑∏∫
Accents: naïve résumé
EOF

echo ""
echo "If characters appear correctly above and in the TUI, UTF-8 handling is working."
echo ""
echo "Press Ctrl+C to exit the TUI after testing."

echo ""
echo "Run the TUI and paste the characters above to verify rendering:"
echo "  cargo run -- --seconds 3 --lang en --codex-cmd cat"
