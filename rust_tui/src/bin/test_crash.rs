use ratatui::{
    text::{Line, Text},
    widgets::Paragraph,
};

fn main() {
    println!("Testing ratatui text wrapping with problematic strings...\n");

    let test_strings = [
        "Normal text",
        "0;0;0uTesting testing",
        "\x1b[>0;0;0uThis is a test",
        "Testing.\n",
        "",
        "  ",
        "\0Hidden null",
    ];

    for (i, test) in test_strings.iter().enumerate() {
        println!("Test {}: {:?}", i + 1, test);

        // Try to create a paragraph with wrapping
        let line = Line::from(*test);
        let text = Text::from(vec![line]);
        let _paragraph = Paragraph::new(text);

        // If we get here without panic, the test passed
        println!("  ✓ No crash\n");
    }

    println!("All tests completed!");
}
