use std::collections::VecDeque;

pub const MAX_TEXT_BYTES: usize = 1024 * 1024;

pub struct History {
    pub items: VecDeque<String>,
    limit: usize,
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self {
            items: VecDeque::new(),
            limit,
        }
    }

    pub fn resize(&mut self, limit: usize) {
        self.limit = limit;
        self.items.truncate(limit);
    }

    pub fn push(&mut self, text: String) {
        if text.is_empty() || text.len() > MAX_TEXT_BYTES {
            return;
        }
        self.items.retain(|item| item != &text);
        self.items.push_front(text);
        self.items.truncate(self.limit);
    }
}

pub fn preview(text: &str) -> String {
    let mut result: String = text
        .chars()
        .take(160)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if text.chars().nth(160).is_some() {
        result.push('…');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recent_unique_and_bounded() {
        let mut h = History::new(2);
        for s in ["one", "two", "one", "three"] {
            h.push(s.into());
        }
        assert_eq!(h.items, ["three", "one"]);
        h.push(String::new());
        h.push("x".repeat(MAX_TEXT_BYTES + 1));
        assert_eq!(h.items.len(), 2);
        h.resize(1);
        assert_eq!(h.items, ["three"]);
    }
    #[test]
    fn preview_preserves_unicode_without_newlines() {
        assert_eq!(preview("hello\r\n世界"), "hello  世界");
        assert_eq!(preview(&"😀".repeat(161)).chars().count(), 161);
    }
}
