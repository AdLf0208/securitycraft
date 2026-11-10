// The keypad's access-control rules, pure and unit-tested.
//
// Upstream (`KeypadBlock`, `KeypadBlockEntity`, `IPasscodeProtected`,
// `CheckPasscode`, `SetPasscode`, `PasscodeUtils`) keeps a salted hash of the
// typed digits, verifies a submitted entry against it, and on a match cycles
// the keypad's `POWERED` state for `signalLength` ticks, which is what opens a
// reinforced door beside it through ordinary redstone. This module holds the
// parts of that which need no game: the entry, the digest, the verdict and the
// signal countdown. The glue in `support.rs` reads and writes the world.
//
// Upstream double-hashes (SHA-512 on the client, PBKDF2-HMAC-SHA1 with 65,536
// rounds and a random 16-byte salt on the server). The port keeps a salted
// SHA-256 digest: the same shape (a random salt per keypad, a one-way digest,
// the plain digits never saved) without a key-derivation loop on the server
// thread. Recorded as a deliberate deviation.

use sha2::{Digest, Sha256};

/// The longest entry the keypad accepts. Upstream's text box has no limit, but
/// its legacy passcodes were at most 20 characters; beyond that typos are the
/// only thing a longer code buys.
pub const MAX_CODE_LEN: usize = 20;

/// The default time the keypad stays powered after a correct code
/// (`SignalLengthOption(60)`, 60 ticks = 3 s).
pub const DEFAULT_SIGNAL_TICKS: i32 = 60;

/// Menu-button ids the keypad screen sends. 0..=9 are the digits, so the two
/// other keys start above them.
pub const BUTTON_BACKSPACE: i32 = 10;
pub const BUTTON_SUBMIT: i32 = 11;

/// A key on the keypad screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Digit(u8),
    Backspace,
    Submit,
}

/// The key a menu-button id stands for, `None` for an id the screen never
/// sends.
#[must_use]
pub fn button(id: i32) -> Option<Button> {
    match id {
        0..=9 => Some(Button::Digit(u8::try_from(id).ok()?)),
        BUTTON_BACKSPACE => Some(Button::Backspace),
        BUTTON_SUBMIT => Some(Button::Submit),
        _ => None,
    }
}

/// Whether `code` is something a player could type: one to [`MAX_CODE_LEN`]
/// ASCII digits. Upstream's text box filters to `\d*`, so letters never reach
/// the server.
#[must_use]
pub fn valid_code(code: &str) -> bool {
    !code.is_empty() && code.len() <= MAX_CODE_LEN && code.bytes().all(|byte| byte.is_ascii_digit())
}

/// The hex digest a keypad stores: SHA-256 over the salt's bytes followed by
/// the code's digits. `None` when the code or the salt is malformed, so a
/// corrupt save cannot become a code that matches everything.
#[must_use]
pub fn hash_code(code: &str, salt_hex: &str) -> Option<String> {
    if !valid_code(code) {
        return None;
    }
    let salt = decode_hex(salt_hex)?;
    let mut digest = Sha256::new();
    digest.update(&salt);
    digest.update(code.as_bytes());
    Some(encode_hex(&digest.finalize()))
}

/// A salt as the hex a keypad saves, from the bytes the level's random gave.
#[must_use]
pub fn new_salt(bytes: &[u8]) -> String {
    encode_hex(bytes)
}

/// What a submitted entry means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// No code has been set on this keypad yet (`getPasscode() == null`).
    NotSet,
    /// Nothing was typed; upstream's check screen sends it and the server's
    /// comparison fails.
    Empty,
    /// The digest matches the stored one.
    Correct,
    /// Anything else.
    Incorrect,
}

/// Compares the typed `entry` with the keypad's stored digest.
#[must_use]
pub fn submit(entry: &str, salt_hex: &str, stored: Option<&str>) -> Verdict {
    let Some(stored) = stored else {
        return Verdict::NotSet;
    };
    if entry.is_empty() {
        return Verdict::Empty;
    }
    match hash_code(entry, salt_hex) {
        Some(digest) if digest == stored => Verdict::Correct,
        _ => Verdict::Incorrect,
    }
}

/// The digits typed into an open keypad screen since it opened.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    text: String,
}

impl Entry {
    /// An empty entry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The digits typed so far.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// How many digits were typed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Whether no digit was typed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Adds one digit, refusing a tenth-plus digit and a non-digit.
    pub fn push(&mut self, digit: u8) -> bool {
        if digit > 9 || self.len() >= MAX_CODE_LEN {
            return false;
        }
        self.text.push(char::from(b'0' + digit));
        true
    }

    /// Removes the last digit; false when there was none.
    pub fn backspace(&mut self) -> bool {
        self.text.pop().is_some()
    }

    /// Forgets every digit, as opening the screen or submitting does.
    pub fn clear(&mut self) {
        self.text.clear();
    }

    /// The code display: one `*` per typed digit, never the digits themselves
    /// (upstream's censoring text box unless "show passcode" is ticked).
    #[must_use]
    pub fn masked(&self) -> String {
        "*".repeat(self.len())
    }
}

/// The direction the keypad's face points when a player with `yaw` places it:
/// the opposite of the way they look, as upstream's
/// `getNearestLookingDirection().getOpposite()` does. Vanilla's quarter turns
/// are `floor(yaw / 90 + 0.5) mod 4` = south, west, north, east.
#[must_use]
pub fn facing_from_yaw(yaw: f32) -> &'static str {
    let quarter = ((yaw / 90.0 + 0.5).floor() as i64).rem_euclid(4);
    ["north", "east", "south", "west"][usize::try_from(quarter).unwrap_or_default()]
}

/// Whether the open keypad screen checks an entered code or sets the first one
/// (`openPasscodeGUI` vs `openSetPasscodeScreen`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Ask for the code (a code is set).
    Check,
    /// Ask for a code to store (the keypad has none).
    Set,
}

impl Mode {
    /// The mode a saved `mode` field names; anything but `1` is check.
    #[must_use]
    pub fn from_int(value: i32) -> Self {
        if value == 1 { Self::Set } else { Self::Check }
    }

    /// The mode as the block entity saves it.
    #[must_use]
    pub fn code(self) -> i32 {
        match self {
            Self::Check => 0,
            Self::Set => 1,
        }
    }
}

/// What the keypad does with a submitted entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// The code matched: power the keypad.
    Open,
    /// The code did not match.
    Wrong,
    /// A valid new code was typed in set mode; store its digest.
    Set,
    /// Enter was pressed with nothing typed.
    Empty,
    /// A check on a keypad with no code becomes a set.
    Check,
}

/// Decides what Enter means, from the screen's mode, the typed entry, the
/// keypad's salt and its stored digest. Upstream: the set screen saves any
/// non-empty digit string, and the check screen compares the digest.
#[must_use]
pub fn after_submit(mode: Mode, entry: &str, salt_hex: &str, stored: Option<&str>) -> Action {
    if entry.is_empty() {
        return Action::Empty;
    }
    match mode {
        Mode::Set => {
            if valid_code(entry) {
                Action::Set
            } else {
                Action::Empty
            }
        }
        Mode::Check => match submit(entry, salt_hex, stored) {
            Verdict::Correct => Action::Open,
            Verdict::Incorrect | Verdict::Empty => Action::Wrong,
            Verdict::NotSet => Action::Check,
        },
    }
}

/// One tick of the keypad's power countdown: the ticks left and whether the
/// keypad turns its signal off now. Upstream schedules a block tick at
/// `signalLength` when it powers on and clears `POWERED` there.
#[must_use]
pub fn tick_signal(remaining: i32) -> (i32, bool) {
    if remaining <= 0 {
        return (0, false);
    }
    let next = remaining - 1;
    (next, next == 0)
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().chunks_exact(2) {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        bytes.push(high << 4 | low);
    }
    Some(bytes)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The salt the in-game path draws is 16 bytes; tests use a fixed one.
    const SALT: &str = "000102030405060708090a0b0c0d0e0f";
    const OTHER_SALT: &str = "ffeeddccbbaa99887766554433221100";

    #[test]
    fn a_code_is_one_to_twenty_ascii_digits() {
        assert!(valid_code("1234"));
        assert!(valid_code("0"));
        assert!(valid_code(&"9".repeat(MAX_CODE_LEN)));
        assert!(!valid_code(""));
        assert!(!valid_code(&"9".repeat(MAX_CODE_LEN + 1)));
        assert!(!valid_code("12a4"));
        assert!(!valid_code("12 4"));
        assert!(!valid_code("-123"));
    }

    #[test]
    fn the_digest_is_sha256_of_the_salt_then_the_code() {
        // sha256(bytes.fromhex(SALT) + b"1234")
        assert_eq!(
            hash_code("1234", SALT).as_deref(),
            Some("bad0b4f7cae08eb7c1b5acc763a8ed25242e2c9173e81f3f624b1fa5303cf6ce")
        );
    }

    #[test]
    fn the_same_code_under_another_salt_has_another_digest() {
        assert_ne!(hash_code("1234", SALT), hash_code("1234", OTHER_SALT));
        assert_eq!(
            hash_code("1234", OTHER_SALT).as_deref(),
            Some("3c98a0ee8cde8b39aca8950f017c0e607e1c274706e665c492b6e903f886cf43")
        );
    }

    #[test]
    fn a_different_code_under_the_same_salt_has_another_digest() {
        assert_ne!(hash_code("1234", SALT), hash_code("4321", SALT));
    }

    #[test]
    fn an_empty_code_has_no_digest() {
        assert_eq!(hash_code("", SALT), None);
    }

    #[test]
    fn a_malformed_salt_has_no_digest() {
        assert_eq!(hash_code("1234", "abc"), None);
        assert_eq!(hash_code("1234", "zz"), None);
        assert_eq!(hash_code("1234", ""), Some(hash_code("1234", "").unwrap()));
    }

    #[test]
    fn a_keypad_without_a_code_says_so() {
        assert_eq!(submit("1234", SALT, None), Verdict::NotSet);
    }

    #[test]
    fn an_empty_entry_is_never_correct() {
        assert_eq!(
            submit("", SALT, hash_code("1234", SALT).as_deref()),
            Verdict::Empty
        );
    }

    #[test]
    fn the_right_code_matches() {
        let stored = hash_code("1234", SALT).unwrap();
        assert_eq!(submit("1234", SALT, Some(&stored)), Verdict::Correct);
    }

    #[test]
    fn any_other_code_is_incorrect() {
        let stored = hash_code("1234", SALT).unwrap();
        assert_eq!(submit("4321", SALT, Some(&stored)), Verdict::Incorrect);
        assert_eq!(submit("123", SALT, Some(&stored)), Verdict::Incorrect);
        assert_eq!(submit("12345", SALT, Some(&stored)), Verdict::Incorrect);
    }

    #[test]
    fn a_digest_from_another_salt_does_not_match() {
        let stored = hash_code("1234", OTHER_SALT).unwrap();
        assert_eq!(submit("1234", SALT, Some(&stored)), Verdict::Incorrect);
    }

    #[test]
    fn entry_collects_digits_in_order() {
        let mut entry = Entry::new();
        assert!(entry.push(1));
        assert!(entry.push(2));
        assert_eq!(entry.text(), "12");
        assert_eq!(entry.len(), 2);
        assert!(!entry.is_empty());
    }

    #[test]
    fn entry_stops_at_the_code_limit() {
        let mut entry = Entry::new();
        for digit in 0..MAX_CODE_LEN {
            assert!(entry.push(u8::try_from(digit % 10).unwrap()));
        }
        assert!(!entry.push(7));
        assert_eq!(entry.len(), MAX_CODE_LEN);
    }

    #[test]
    fn entry_refuses_a_non_digit() {
        let mut entry = Entry::new();
        assert!(!entry.push(10));
        assert!(entry.is_empty());
    }

    #[test]
    fn backspace_removes_the_last_digit_only() {
        let mut entry = Entry::new();
        entry.push(1);
        entry.push(2);
        assert!(entry.backspace());
        assert_eq!(entry.text(), "1");
        assert!(entry.backspace());
        assert!(!entry.backspace());
        assert!(entry.is_empty());
    }

    #[test]
    fn clear_forgets_everything() {
        let mut entry = Entry::new();
        entry.push(4);
        entry.push(2);
        entry.clear();
        assert!(entry.is_empty());
    }

    #[test]
    fn the_display_hides_the_digits() {
        let mut entry = Entry::new();
        entry.push(9);
        entry.push(1);
        entry.push(2);
        assert_eq!(entry.masked(), "***");
        assert_eq!(Entry::new().masked(), "");
    }

    #[test]
    fn the_signal_counts_down_and_turns_off_once() {
        assert_eq!(tick_signal(3), (2, false));
        assert_eq!(tick_signal(2), (1, false));
        assert_eq!(tick_signal(1), (0, true));
        assert_eq!(tick_signal(0), (0, false));
        assert_eq!(tick_signal(-4), (0, false));
    }

    #[test]
    fn the_screen_keys_map_to_digits_and_the_two_actions() {
        assert_eq!(button(0), Some(Button::Digit(0)));
        assert_eq!(button(9), Some(Button::Digit(9)));
        assert_eq!(button(BUTTON_BACKSPACE), Some(Button::Backspace));
        assert_eq!(button(BUTTON_SUBMIT), Some(Button::Submit));
        assert_eq!(button(12), None);
        assert_eq!(button(-1), None);
    }

    #[test]
    fn the_keypad_faces_the_player_who_places_it() {
        assert_eq!(facing_from_yaw(0.0), "north");
        assert_eq!(facing_from_yaw(90.0), "east");
        assert_eq!(facing_from_yaw(180.0), "south");
        assert_eq!(facing_from_yaw(-90.0), "west");
        assert_eq!(facing_from_yaw(360.0), "north");
    }

    #[test]
    fn modes_round_trip_through_the_saved_int() {
        assert_eq!(Mode::from_int(1), Mode::Set);
        assert_eq!(Mode::from_int(0), Mode::Check);
        assert_eq!(Mode::from_int(7), Mode::Check);
        assert_eq!(Mode::Set.code(), 1);
        assert_eq!(Mode::Check.code(), 0);
    }

    #[test]
    fn set_mode_stores_any_valid_code() {
        assert_eq!(after_submit(Mode::Set, "2468", SALT, None), Action::Set);
    }

    #[test]
    fn set_mode_with_nothing_typed_does_nothing() {
        assert_eq!(after_submit(Mode::Set, "", SALT, None), Action::Empty);
    }

    #[test]
    fn checking_an_unset_keypad_switches_to_set() {
        assert_eq!(after_submit(Mode::Check, "1234", SALT, None), Action::Check);
    }

    #[test]
    fn a_matching_code_opens_the_door() {
        let stored = hash_code("1234", SALT).unwrap();
        assert_eq!(
            after_submit(Mode::Check, "1234", SALT, Some(&stored)),
            Action::Open
        );
    }

    #[test]
    fn a_wrong_code_does_nothing() {
        let stored = hash_code("1234", SALT).unwrap();
        assert_eq!(
            after_submit(Mode::Check, "9999", SALT, Some(&stored)),
            Action::Wrong
        );
    }

    #[test]
    fn enter_with_nothing_typed_is_empty() {
        let stored = hash_code("1234", SALT).unwrap();
        assert_eq!(
            after_submit(Mode::Check, "", SALT, Some(&stored)),
            Action::Empty
        );
    }

    #[test]
    fn a_salt_round_trips_through_the_saved_hex() {
        let bytes = [0x00, 0x0f, 0xa5, 0xff];
        assert_eq!(new_salt(&bytes), "000fa5ff");
    }
}
