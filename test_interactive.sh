#!/bin/bash
# Test Codex interactive mode
echo "Testing Codex interactive mode..."

# Create a test input file
cat > /tmp/test_input.txt << 'EOF'
Hello
What is 2+2?
exit
EOF

# Run codex in interactive mode with test input
cd /Users/jguida941/new_github_projects/codex_voice
codex < /tmp/test_input.txt 2>&1 | head -50