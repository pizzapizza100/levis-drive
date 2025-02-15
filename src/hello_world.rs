/**
 * Returns a greeting message.
 *
 * # Examples
 *
 * ```
 * use levis_drive::hello_world::greet;
 *
 * // The default greeting should be "Hello, world!"
 * assert_eq!(greet(), "Hello, world!");
 * ```
 */
pub fn greet() -> &'static str {
    "Hello, world!"
}

/**
 * Returns a personalized greeting message.
 *
 * # Examples
 *
 * ```
 * use levis_drive::hello_world::greet_name;
 *
 * // Greeting with a name returns "Hello, <name>!"
 * let message = greet_name("Bob");
 * assert_eq!(message, "Hello, Bob!");
 * ```
 */
pub fn greet_name(name: &str) -> String {
    format!("Hello, {}!", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greet() {
        // Test the default greeting
        assert_eq!(greet(), "Hello, world!");
    }

    #[test]
    fn test_greet_name() {
        // Test greeting a specific name
        assert_eq!(greet_name("Charlie"), "Hello, Charlie!");
    }

    #[test]
    fn test_greet_name_empty() {
        // Extra check: Ensure that greeting with an empty string works as expected.
        // (Depending on requirements, you might want to handle this differently.)
        assert_eq!(greet_name(""), "Hello, !");
    }
}
