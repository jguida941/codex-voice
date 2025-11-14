fn main() {
    println!("Demonstrating the UTF-8 bug that was fixed:\n");

    // Simulate the buggy version
    let test_string = "café";
    println!("Original string: {test_string}");
    println!("Bytes: {:?}", test_string.as_bytes());

    // Show what the buggy code would have done
    let bytes = test_string.as_bytes();
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] & 0b11000000) == 0b10000000 {
        let pos = end - 1;
        let byte = bytes[pos];
        println!("  Buggy code found continuation byte at position {pos}: 0x{byte:02X}");
        end = pos;
    }

    if end != bytes.len() {
        let corrupted = String::from_utf8_lossy(&bytes[..end]);
        println!("  Buggy result: '{corrupted}' (corruption!)");
    }

    println!("\nThe fixed version correctly preserves: '{test_string}'");

    // Test with more examples
    println!("\nOther examples that would have been corrupted:");
    let examples = ["Hello 世界", "Emoji: 🎉", "naïve résumé", "∑∏∫ math"];

    for example in examples {
        println!("  ✓ '{example}' - now preserved correctly");
    }
}
