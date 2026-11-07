//! Passwords, user names and account lists.

use anyhow::Result;
use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::Rng;

use crate::db::{Account, Role};

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("hashing password: {e}"))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

/// 8 characters without look-alikes (0/O, 1/l/I), easy to type from a slip.
pub fn generate_password() -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    (0..8).map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char).collect()
}

/// Random token for sessions (hex, 256 bits).
pub fn generate_token() -> String {
    let bytes: [u8; 32] = rand::thread_rng().gen();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// User names double as folder names in `source/`: letters, digits, `-`,
/// `_`, `.`; 1-32 characters; not starting with a dot.
pub fn valid_username(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && !s.starts_with('.')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !matches!(s.to_ascii_lowercase().as_str(), "con" | "prn" | "aux" | "nul")
}

/// One line of an account list: `username,display name,group,seat[,password]`
/// (a header line starting with `username` is skipped).
pub fn parse_account_csv(text: &str) -> Result<Vec<(Account, Option<String>)>> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<String> = split_csv(line);
        if i == 0 && f.first().is_some_and(|h| h.eq_ignore_ascii_case("username")) {
            continue;
        }
        let get = |k: usize| f.get(k).map(|s| s.trim().to_string()).unwrap_or_default();
        let username = get(0);
        anyhow::ensure!(valid_username(&username), "line {}: invalid user name {:?}", i + 1, username);
        let a = Account {
            id: 0,
            username,
            display_name: get(1),
            group_name: get(2),
            seat: get(3),
            role: Role::Contestant,
            ip_lock: String::new(),
            disabled: false,
        };
        let pw = get(4);
        out.push((a, (!pw.is_empty()).then_some(pw)));
    }
    Ok(out)
}

fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords() {
        let p = generate_password();
        assert_eq!(p.len(), 8);
        let h = hash_password(&p).unwrap();
        assert!(verify_password(&p, &h));
        assert!(!verify_password("wrong", &h));
        assert!(!verify_password(&p, "garbage"));
    }

    #[test]
    fn usernames_and_csv() {
        assert!(valid_username("team-01_a.b"));
        for bad in ["", "..", ".x", "a/b", "a b", "con", "张三"] {
            assert!(!valid_username(bad), "{bad}");
        }
        let v = parse_account_csv("username,name,school,seat\nt1,\"Team, One\",S1,A-01\nt2,Two,,B-02,secret\n").unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].0.display_name, "Team, One");
        assert_eq!(v[1].1.as_deref(), Some("secret"));
        assert!(parse_account_csv("bad name,x").is_err());
    }
}
