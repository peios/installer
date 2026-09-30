//! What the account page is answered with, checked before setup goes on.
//!
//! The account is made at the very end, by `lps add`, and lpsd refuses a
//! name or a password it will not store. Refused there, it fails the whole
//! of setup, which then runs again on the next boot: so what lpsd would
//! refuse is refused here instead, on the page, where it can be put right.
//!
//! lpsd's `check_name` is the authority on names, and this follows it:
//! printable ASCII, no longer than it keeps, none of the characters it
//! reserves, and none of the well-known groups. It trims the name, and so
//! does this.

use crate::setup::VISITOR;

/// The longest name lpsd keeps, in bytes.
const MAX_NAME_BYTES: usize = 256;

/// What lpsd will not have in a name: each is a separator somewhere a name
/// ends up (a qualified name, a path, `/etc/passwd`).
const RESERVED: &[char] = &['@', '\\', '/', ':', ','];

/// The groups every machine has, whose names lpsd keeps for them.
const WELL_KNOWN: &[&str] = &[
    "Everyone",
    "Authenticated Users",
    "Administrators",
    "Users",
    "Guests",
];

/// What the account page asks for, as answered.
pub struct Answered<'a> {
    pub name: &'a str,
    pub password: &'a str,
    pub confirm: &'a str,
}

/// The account's name, trimmed, if everything answered can be made into an
/// account; otherwise what is wrong, by field, all of it at once.
pub fn check(answered: Answered) -> Result<String, Vec<(String, String)>> {
    let mut wrong = Vec::new();
    let name = answered.name.trim();
    if let Err(why) = check_name(name) {
        wrong.push(("account.name".to_string(), why));
    }
    if answered.password.is_empty() {
        wrong.push((
            "account.password".to_string(),
            "Choose a password. This account administers the machine.".to_string(),
        ));
    } else if answered.password != answered.confirm {
        // On confirm, not on password: the person retypes the one they got
        // wrong, and the first field keeps what they meant.
        wrong.push((
            "account.confirm".to_string(),
            "The passwords do not match.".to_string(),
        ));
    }
    if wrong.is_empty() {
        Ok(name.to_string())
    } else {
        Err(wrong)
    }
}

fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Give the account a name.".into());
    }
    if let Some(c) = name.chars().find(|c| !(' '..='~').contains(c)) {
        return Err(if c.is_control() {
            "A name cannot contain control characters.".into()
        } else {
            format!(
                "A name is unaccented letters, digits, spaces and punctuation for now; “{c}” is not one of them."
            )
        });
    }
    // Being ASCII, its bytes are its characters.
    if name.len() > MAX_NAME_BYTES {
        return Err(format!("A name is at most {MAX_NAME_BYTES} characters."));
    }
    if let Some(c) = name.chars().find(|c| RESERVED.contains(c)) {
        return Err(format!("A name cannot contain “{c}”."));
    }
    let bare = name.strip_prefix("BUILTIN\\").unwrap_or(name);
    if let Some(group) = WELL_KNOWN
        .iter()
        .find(|group| group.eq_ignore_ascii_case(bare))
    {
        return Err(format!(
            "{group} is a group every Peios machine has. Choose another name."
        ));
    }
    // Setup keeps an account that already exists, and this one does while
    // setup runs: taking its name would finish setup with no account the
    // person can use, and remove it besides.
    if name.eq_ignore_ascii_case(VISITOR) {
        return Err("That name is used by setup itself. Choose another.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answered<'a>(
        name: &'a str,
        password: &'a str,
        confirm: &'a str,
    ) -> Result<String, Vec<(String, String)>> {
        check(Answered {
            name,
            password,
            confirm,
        })
    }

    fn refs(result: Result<String, Vec<(String, String)>>) -> Vec<String> {
        result.unwrap_err().into_iter().map(|(r, _)| r).collect()
    }

    #[test]
    fn a_name_and_a_confirmed_password_make_an_account() {
        assert_eq!(answered("jack", "hunter2", "hunter2").unwrap(), "jack");
        // Trimmed, as lpsd trims it, and interior spaces kept.
        assert_eq!(
            answered("  Jack Palfrey ", "x", "x").unwrap(),
            "Jack Palfrey"
        );
        // Punctuation lpsd does not reserve is fine.
        assert_eq!(
            answered("j.palfrey-2_x", "x", "x").unwrap(),
            "j.palfrey-2_x"
        );
    }

    #[test]
    fn what_lpsd_would_refuse_is_refused_on_the_page() {
        for name in [
            "",
            "   ",
            "jack@home",
            "DOMAIN\\jack",
            "a/b",
            "a:b",
            "a,b",
            "Jäck",
            "tab\there",
        ] {
            assert_eq!(refs(answered(name, "x", "x")), ["account.name"], "{name:?}");
        }
        assert_eq!(refs(answered(&"a".repeat(257), "x", "x")), ["account.name"]);
        assert!(answered(&"a".repeat(256), "x", "x").is_ok());
        for name in [
            "Administrators",
            "administrators",
            "Everyone",
            "authenticated users",
            "BUILTIN\\Users",
        ] {
            assert_eq!(refs(answered(name, "x", "x")), ["account.name"], "{name:?}");
        }
        // Names that only contain a group's name are someone's.
        assert!(answered("Guestsroom", "x", "x").is_ok());
    }

    #[test]
    fn setup_s_own_account_is_not_offered() {
        assert_eq!(refs(answered(VISITOR, "x", "x")), ["account.name"]);
        assert_eq!(
            refs(answered(&VISITOR.to_uppercase(), "x", "x")),
            ["account.name"]
        );
    }

    #[test]
    fn a_password_is_needed_and_must_be_typed_twice_alike() {
        assert_eq!(refs(answered("jack", "", "")), ["account.password"]);
        assert_eq!(refs(answered("jack", "one", "two")), ["account.confirm"]);
        // A password is not trimmed: a space is a character like any other.
        assert!(answered("jack", " ", " ").is_ok());
        assert_eq!(refs(answered("jack", "one ", "one")), ["account.confirm"]);
    }

    #[test]
    fn everything_wrong_is_said_at_once() {
        assert_eq!(
            refs(answered("a@b", "one", "two")),
            ["account.name", "account.confirm"]
        );
    }
}
