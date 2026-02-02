#!/bin/bash
# Quick script to find all potential underflow sites

echo "=== Looking for signed type arithmetic with width/cursor ==="
rg -n "(isize|i32|i64|i16|i8).*[-].*?(width|cursor|offset|scroll|col)" src/

echo ""
echo "=== Looking for 'as usize' casts (potential underflow sites) ==="
rg -n "as usize" src/ | grep -v "// safe" | grep -v "MAX"

echo ""
echo "=== Looking for raw slicing patterns [..] ==="
rg -n "\[\.\.|\.\.\]|\.\.\=" src/ | grep -v "get(" | grep -v "test"

echo ""
echo "=== Looking for subtraction operations that might underflow ==="
rg -n "\w+\s*-\s*\w+" src/ | grep -E "(cursor|width|offset|scroll|col|pos|idx|index)"

echo ""
echo "=== Checking for any .wrap() calls that might still be active ==="
rg -n "\.wrap\(" src/ | grep -v "wrap(Wrap { trim: false })"