//! Single-line status display for CLI startup.

/// Single-line status display that overwrites itself
pub struct StatusLine {
    last_len: usize,
}

impl StatusLine {
    pub fn new() -> Self {
        Self { last_len: 0 }
    }

    /// Update the status line (overwrites previous content)
    pub fn update(&mut self, message: &str) {
        use colored::Colorize;
        use std::io::{self, Write};

        // Suppress output when stdout is piped
        if !crate::output::is_interactive() {
            return;
        }

        // Clear previous line
        print!("\r{}\r", " ".repeat(self.last_len));

        // Print new message with spinner
        let formatted = format!("{} {}", "⟳".cyan(), message);
        print!("{}", formatted);
        io::stdout().flush().unwrap();

        self.last_len = formatted.len() + 5; // Extra padding for safety
    }

    /// Complete with a success message
    pub fn success(&mut self, message: &str) {
        use colored::Colorize;
        use std::io::{self, Write};

        // Suppress output when stdout is piped
        if !crate::output::is_interactive() {
            return;
        }

        // Clear previous line
        print!("\r{}\r", " ".repeat(self.last_len));

        // Print success message
        println!("{} {}", "✓".green(), message);
        io::stdout().flush().unwrap();

        self.last_len = 0;
    }
}

impl Default for StatusLine {
    fn default() -> Self {
        Self::new()
    }
}
