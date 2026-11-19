use corpus_recorder::{Interface, Key};
use std::io;

#[cfg(any(windows, test))]
struct KeyFilter {
    pressed: [bool; 256],
}

#[cfg(any(windows, test))]
impl Default for KeyFilter {
    fn default() -> Self {
        Self {
            pressed: [false; 256],
        }
    }
}

#[cfg(any(windows, test))]
impl KeyFilter {
    fn decode(
        &mut self,
        virtual_key: u16,
        character: u16,
        down: bool,
        control: bool,
        alt: bool,
    ) -> Option<Key> {
        let pressed = self.pressed.get_mut(usize::from(virtual_key))?;
        let already_pressed = *pressed;
        *pressed = down;
        if !down || already_pressed {
            return None;
        }
        if control && !alt && virtual_key == 0x43 {
            return Some(Key::Quit);
        }
        if control || alt {
            return Some(Key::Other);
        }
        Some(match virtual_key {
            0x0d => Key::Enter,
            0x1b => Key::Quit,
            _ => match char::from_u32(u32::from(character)).map(|value| value.to_ascii_lowercase())
            {
                Some('w') => Key::Repeat,
                Some('s') => Key::Skip,
                _ => Key::Other,
            },
        })
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows::Win32::{
        Foundation::HANDLE,
        System::Console::{
            CONSOLE_MODE, ENABLE_ECHO_INPUT, ENABLE_EXTENDED_FLAGS, ENABLE_LINE_INPUT,
            ENABLE_PROCESSED_INPUT, ENABLE_QUICK_EDIT_MODE, ENABLE_VIRTUAL_TERMINAL_INPUT,
            FOCUS_EVENT, GetConsoleMode, GetStdHandle, INPUT_RECORD, KEY_EVENT, LEFT_ALT_PRESSED,
            LEFT_CTRL_PRESSED, RIGHT_ALT_PRESSED, RIGHT_CTRL_PRESSED, ReadConsoleInputW,
            STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleMode, WriteConsoleW,
        },
    };

    pub struct Terminal {
        input: HANDLE,
        output: HANDLE,
        original_mode: CONSOLE_MODE,
        keys: KeyFilter,
    }

    impl Terminal {
        pub fn open() -> io::Result<Self> {
            // Borrowed standard handles remain owned by the process.
            let input =
                unsafe { GetStdHandle(STD_INPUT_HANDLE) }.map_err(|_| console_required())?;
            let output =
                unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }.map_err(|_| console_required())?;
            let mut original_mode = CONSOLE_MODE::default();
            let mut output_mode = CONSOLE_MODE::default();
            unsafe {
                GetConsoleMode(input, &mut original_mode).map_err(|_| console_required())?;
                GetConsoleMode(output, &mut output_mode).map_err(|_| console_required())?;
            }
            let mode = (original_mode | ENABLE_EXTENDED_FLAGS)
                & !(ENABLE_ECHO_INPUT
                    | ENABLE_LINE_INPUT
                    | ENABLE_PROCESSED_INPUT
                    | ENABLE_QUICK_EDIT_MODE
                    | ENABLE_VIRTUAL_TERMINAL_INPUT);
            unsafe { SetConsoleMode(input, mode) }.map_err(|_| {
                io::Error::other("Die Terminaleingabe konnte nicht vorbereitet werden.")
            })?;
            Ok(Self {
                input,
                output,
                original_mode,
                keys: KeyFilter::default(),
            })
        }
    }

    impl Interface for Terminal {
        fn write(&mut self, message: &str) -> io::Result<()> {
            let text: Vec<_> = message.encode_utf16().collect();
            let mut remaining = text.as_slice();
            while !remaining.is_empty() {
                let mut written = 0;
                let chunk = &remaining[..remaining.len().min(8_192)];
                unsafe { WriteConsoleW(self.output, chunk, Some(&mut written), None) }.map_err(
                    |_| io::Error::other("Die Terminalausgabe ist nicht mehr verfügbar."),
                )?;
                if written == 0 || written as usize > chunk.len() {
                    return Err(io::Error::other("Die Terminalausgabe wurde unterbrochen."));
                }
                remaining = &remaining[written as usize..];
            }
            Ok(())
        }

        fn read_key(&mut self) -> io::Result<Key> {
            loop {
                let mut records = [INPUT_RECORD::default()];
                let mut count = 0;
                unsafe { ReadConsoleInputW(self.input, &mut records, &mut count) }.map_err(
                    |_| io::Error::other("Die Terminaleingabe ist nicht mehr verfügbar."),
                )?;
                if count == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Die Terminaleingabe wurde beendet.",
                    ));
                }
                let record = records[0];
                if u32::from(record.EventType) == FOCUS_EVENT {
                    self.keys = KeyFilter::default();
                }
                if u32::from(record.EventType) != KEY_EVENT {
                    continue;
                }
                // EventType selects the active member of the Windows input union.
                let event = unsafe { record.Event.KeyEvent };
                let character = unsafe { event.uChar.UnicodeChar };
                if let Some(key) = self.keys.decode(
                    event.wVirtualKeyCode,
                    character,
                    event.bKeyDown.as_bool(),
                    event.dwControlKeyState & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0,
                    event.dwControlKeyState & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED) != 0,
                ) {
                    return Ok(key);
                }
            }
        }
    }

    impl Drop for Terminal {
        fn drop(&mut self) {
            let _ = unsafe { SetConsoleMode(self.input, self.original_mode) };
        }
    }

    fn console_required() -> io::Error {
        io::Error::other(
            "Bitte den Recorder in einem interaktiven Windows-Terminal ohne Umleitung oder Pipe starten. \
             Satztexte dürfen nicht in Ausgabedateien gelangen.",
        )
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub struct Terminal;

    impl Terminal {
        pub fn open() -> io::Result<Self> {
            Err(io::Error::other(
                "Der Korpus-Recorder benötigt ein Windows-Terminal.",
            ))
        }
    }

    impl Interface for Terminal {
        fn write(&mut self, _: &str) -> io::Result<()> {
            Err(io::Error::other(
                "Der Korpus-Recorder benötigt ein Windows-Terminal.",
            ))
        }

        fn read_key(&mut self) -> io::Result<Key> {
            Err(io::Error::other(
                "Der Korpus-Recorder benötigt ein Windows-Terminal.",
            ))
        }
    }
}

pub use platform::Terminal;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_keys_map_without_a_following_enter() {
        for (virtual_key, character, expected) in [
            (0x0d, '\r', Key::Enter),
            (0x57, 'w', Key::Repeat),
            (0x57, 'W', Key::Repeat),
            (0x53, 's', Key::Skip),
            (0x53, 'S', Key::Skip),
            (0x51, 'q', Key::Other),
            (0x1b, '\u{1b}', Key::Quit),
            (0x41, 'a', Key::Other),
        ] {
            let mut filter = KeyFilter::default();
            assert_eq!(
                filter.decode(virtual_key, character as u16, true, false, false),
                Some(expected)
            );
        }
    }

    #[test]
    fn held_enter_cannot_start_and_immediately_stop_a_take() {
        let mut filter = KeyFilter::default();
        assert_eq!(
            filter.decode(0x0d, 13, true, false, false),
            Some(Key::Enter)
        );
        assert_eq!(filter.decode(0x0d, 13, true, false, false), None);
        assert_eq!(filter.decode(0x0d, 13, false, false, false), None);
        assert_eq!(
            filter.decode(0x0d, 13, true, false, false),
            Some(Key::Enter)
        );
    }

    #[test]
    fn ctrl_c_quits_but_other_modifier_shortcuts_are_ignored() {
        let mut filter = KeyFilter::default();
        assert_eq!(filter.decode(0x43, 3, true, true, false), Some(Key::Quit));
        assert_eq!(filter.decode(0x57, 23, true, true, false), Some(Key::Other));
        assert_eq!(
            filter.decode(0x53, b's' as u16, true, false, true),
            Some(Key::Other)
        );
    }

    #[test]
    fn release_and_non_keyboard_codes_do_not_trigger_commands() {
        let mut filter = KeyFilter::default();
        assert_eq!(filter.decode(0x57, b'w' as u16, false, false, false), None);
        assert_eq!(filter.decode(1000, b'w' as u16, true, false, false), None);
    }
}
