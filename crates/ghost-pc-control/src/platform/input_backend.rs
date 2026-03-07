//! Abstraction over input simulation for testability.
//!
//! The `InputBackend` trait provides a uniform interface for mouse and
//! keyboard operations. `EnigoBackend` wraps the `enigo` crate for
//! production use. `MockInputBackend` records actions for unit tests.

use std::sync::{Arc, Mutex};

/// Mouse button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Keyboard key identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Return,
    Tab,
    Escape,
    Backspace,
    Delete,
    Space,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Meta,
    Control,
    Alt,
    Shift,
    F(u8),
}

/// Actions recorded by `MockInputBackend` for test assertions.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordedAction {
    MouseMoveTo(i32, i32),
    MouseClick(MouseButton),
    MouseDoubleClick(MouseButton),
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    KeySequence(String),
    KeyDown(Key),
    KeyUp(Key),
    ScrollY(i32),
    ScrollX(i32),
}

/// Trait abstracting input simulation backends.
///
/// All methods are synchronous, matching the `Skill::execute` contract.
/// `Send` is required so the backend can be wrapped in `Arc<Mutex<>>`.
/// `Sync` is NOT required — the `Mutex` provides synchronization.
pub trait InputBackend: Send {
    fn mouse_move_to(&mut self, x: i32, y: i32);
    fn mouse_click(&mut self, button: MouseButton);
    fn mouse_double_click(&mut self, button: MouseButton);
    fn mouse_down(&mut self, button: MouseButton);
    fn mouse_up(&mut self, button: MouseButton);
    fn key_sequence(&mut self, text: &str);
    fn key_down(&mut self, key: &Key);
    fn key_up(&mut self, key: &Key);
    fn scroll_y(&mut self, length: i32);
    fn scroll_x(&mut self, length: i32);
}

/// Production backend wrapping `enigo` 0.6.
pub struct EnigoBackend {
    enigo: enigo::Enigo,
}

impl EnigoBackend {
    /// Create a new enigo-backed input backend.
    ///
    /// Requires a display server (X11, Quartz, Windows) to be available.
    /// Returns an error if initialization fails (e.g., headless environment).
    pub fn try_new() -> Result<Self, String> {
        let result = std::panic::catch_unwind(|| enigo::Enigo::new(&enigo::Settings::default()));
        match result {
            Ok(Ok(enigo)) => Ok(Self { enigo }),
            Ok(Err(e)) => Err(format!("failed to initialize enigo: {e}")),
            Err(_) => Err("failed to initialize enigo — no display server available".into()),
        }
    }
}

impl InputBackend for EnigoBackend {
    fn mouse_move_to(&mut self, x: i32, y: i32) {
        use enigo::Mouse;
        let _ = self.enigo.move_mouse(x, y, enigo::Coordinate::Abs);
    }

    fn mouse_click(&mut self, button: MouseButton) {
        use enigo::Mouse;
        let _ = self
            .enigo
            .button(to_enigo_button(button), enigo::Direction::Click);
    }

    fn mouse_double_click(&mut self, button: MouseButton) {
        use enigo::Mouse;
        let btn = to_enigo_button(button);
        let _ = self.enigo.button(btn, enigo::Direction::Click);
        let _ = self.enigo.button(btn, enigo::Direction::Click);
    }

    fn mouse_down(&mut self, button: MouseButton) {
        use enigo::Mouse;
        let _ = self
            .enigo
            .button(to_enigo_button(button), enigo::Direction::Press);
    }

    fn mouse_up(&mut self, button: MouseButton) {
        use enigo::Mouse;
        let _ = self
            .enigo
            .button(to_enigo_button(button), enigo::Direction::Release);
    }

    fn key_sequence(&mut self, text: &str) {
        use enigo::Keyboard;
        let _ = self.enigo.text(text);
    }

    fn key_down(&mut self, key: &Key) {
        use enigo::Keyboard;
        let _ = self.enigo.key(to_enigo_key(key), enigo::Direction::Press);
    }

    fn key_up(&mut self, key: &Key) {
        use enigo::Keyboard;
        let _ = self.enigo.key(to_enigo_key(key), enigo::Direction::Release);
    }

    fn scroll_y(&mut self, length: i32) {
        use enigo::Mouse;
        let _ = self.enigo.scroll(length, enigo::Axis::Vertical);
    }

    fn scroll_x(&mut self, length: i32) {
        use enigo::Mouse;
        let _ = self.enigo.scroll(length, enigo::Axis::Horizontal);
    }
}

fn to_enigo_button(button: MouseButton) -> enigo::Button {
    match button {
        MouseButton::Left => enigo::Button::Left,
        MouseButton::Right => enigo::Button::Right,
        MouseButton::Middle => enigo::Button::Middle,
    }
}

fn to_enigo_key(key: &Key) -> enigo::Key {
    match key {
        Key::Char(c) => enigo::Key::Unicode(*c),
        Key::Return => enigo::Key::Return,
        Key::Tab => enigo::Key::Tab,
        Key::Escape => enigo::Key::Escape,
        Key::Backspace => enigo::Key::Backspace,
        Key::Delete => enigo::Key::Delete,
        Key::Space => enigo::Key::Space,
        Key::Up => enigo::Key::UpArrow,
        Key::Down => enigo::Key::DownArrow,
        Key::Left => enigo::Key::LeftArrow,
        Key::Right => enigo::Key::RightArrow,
        Key::Home => enigo::Key::Home,
        Key::End => enigo::Key::End,
        Key::PageUp => enigo::Key::PageUp,
        Key::PageDown => enigo::Key::PageDown,
        Key::Meta => enigo::Key::Meta,
        Key::Control => enigo::Key::Control,
        Key::Alt => enigo::Key::Alt,
        Key::Shift => enigo::Key::Shift,
        Key::F(n) => match n {
            1 => enigo::Key::F1,
            2 => enigo::Key::F2,
            3 => enigo::Key::F3,
            4 => enigo::Key::F4,
            5 => enigo::Key::F5,
            6 => enigo::Key::F6,
            7 => enigo::Key::F7,
            8 => enigo::Key::F8,
            9 => enigo::Key::F9,
            10 => enigo::Key::F10,
            11 => enigo::Key::F11,
            12 => enigo::Key::F12,
            13 => enigo::Key::F13,
            14 => enigo::Key::F14,
            15 => enigo::Key::F15,
            16 => enigo::Key::F16,
            17 => enigo::Key::F17,
            18 => enigo::Key::F18,
            19 => enigo::Key::F19,
            20 => enigo::Key::F20,
            // Fall back to F1 for out-of-range values.
            _ => enigo::Key::F1,
        },
    }
}

/// Mock backend that records all actions for test assertions.
///
/// Thread-safe via internal `Arc<Mutex<>>` so it can be shared with
/// skills that hold `Arc<Mutex<dyn InputBackend>>`.
#[derive(Debug, Clone)]
pub struct MockInputBackend {
    actions: Arc<Mutex<Vec<RecordedAction>>>,
}

impl MockInputBackend {
    pub fn new() -> Self {
        Self {
            actions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get a clone of all recorded actions.
    pub fn actions(&self) -> Vec<RecordedAction> {
        self.actions.lock().unwrap().clone()
    }

    /// Clear recorded actions.
    pub fn clear(&self) {
        self.actions.lock().unwrap().clear();
    }
}

impl Default for MockInputBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl InputBackend for MockInputBackend {
    fn mouse_move_to(&mut self, x: i32, y: i32) {
        self.actions
            .lock()
            .unwrap()
            .push(RecordedAction::MouseMoveTo(x, y));
    }

    fn mouse_click(&mut self, button: MouseButton) {
        self.actions
            .lock()
            .unwrap()
            .push(RecordedAction::MouseClick(button));
    }

    fn mouse_double_click(&mut self, button: MouseButton) {
        self.actions
            .lock()
            .unwrap()
            .push(RecordedAction::MouseDoubleClick(button));
    }

    fn mouse_down(&mut self, button: MouseButton) {
        self.actions
            .lock()
            .unwrap()
            .push(RecordedAction::MouseDown(button));
    }

    fn mouse_up(&mut self, button: MouseButton) {
        self.actions
            .lock()
            .unwrap()
            .push(RecordedAction::MouseUp(button));
    }

    fn key_sequence(&mut self, text: &str) {
        self.actions
            .lock()
            .unwrap()
            .push(RecordedAction::KeySequence(text.to_string()));
    }

    fn key_down(&mut self, key: &Key) {
        self.actions
            .lock()
            .unwrap()
            .push(RecordedAction::KeyDown(key.clone()));
    }

    fn key_up(&mut self, key: &Key) {
        self.actions
            .lock()
            .unwrap()
            .push(RecordedAction::KeyUp(key.clone()));
    }

    fn scroll_y(&mut self, length: i32) {
        self.actions
            .lock()
            .unwrap()
            .push(RecordedAction::ScrollY(length));
    }

    fn scroll_x(&mut self, length: i32) {
        self.actions
            .lock()
            .unwrap()
            .push(RecordedAction::ScrollX(length));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_records_mouse_actions() {
        let mut mock = MockInputBackend::new();
        mock.mouse_move_to(100, 200);
        mock.mouse_click(MouseButton::Left);

        let actions = mock.actions();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0], RecordedAction::MouseMoveTo(100, 200));
        assert_eq!(actions[1], RecordedAction::MouseClick(MouseButton::Left));
    }

    #[test]
    fn mock_records_keyboard_actions() {
        let mut mock = MockInputBackend::new();
        mock.key_sequence("hello");
        mock.key_down(&Key::Control);
        mock.key_down(&Key::Char('c'));
        mock.key_up(&Key::Char('c'));
        mock.key_up(&Key::Control);

        let actions = mock.actions();
        assert_eq!(actions.len(), 5);
        assert_eq!(actions[0], RecordedAction::KeySequence("hello".into()));
    }

    #[test]
    fn mock_clear() {
        let mut mock = MockInputBackend::new();
        mock.mouse_move_to(1, 2);
        assert_eq!(mock.actions().len(), 1);

        mock.clear();
        assert!(mock.actions().is_empty());
    }
}
