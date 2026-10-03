//! Minimal Valve KeyValues (VDF) parser for libraryfolders.vdf / appmanifest.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Vdf {
    Str(String),
    Map(BTreeMap<String, Vdf>),
}

impl Vdf {
    pub fn get(&self, k: &str) -> Option<&Vdf> {
        match self {
            Vdf::Map(m) => m.get(k),
            Vdf::Str(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Vdf::Str(s) => Some(s),
            Vdf::Map(_) => None,
        }
    }

    pub fn as_map(&self) -> Option<&BTreeMap<String, Vdf>> {
        match self {
            Vdf::Map(m) => Some(m),
            Vdf::Str(_) => None,
        }
    }
}

pub fn parse(input: &str) -> Result<Vdf, String> {
    let mut p = Parser {
        s: input.as_bytes(),
        i: 0,
    };
    p.skip_ws();
    p.parse_value()
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if c == b'/' && self.i + 1 < self.s.len() && self.s[self.i + 1] == b'/' {
                while self.i < self.s.len() && self.s[self.i] != b'\n' {
                    self.i += 1;
                }
                continue;
            }
            if c.is_ascii_whitespace() {
                self.i += 1;
                continue;
            }
            break;
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.skip_ws();
        if self.i >= self.s.len() || self.s[self.i] != b'"' {
            return Err("expected quoted string".into());
        }
        self.i += 1;
        let mut out = String::new();
        while self.i < self.s.len() {
            let c = self.s[self.i];
            self.i += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' if self.i < self.s.len() => {
                    let n = self.s[self.i];
                    self.i += 1;
                    out.push(match n {
                        b'n' => '\n',
                        b't' => '\t',
                        b'\\' => '\\',
                        b'"' => '"',
                        other => other as char,
                    });
                }
                _ => out.push(c as char),
            }
        }
        Err("unterminated string".into())
    }

    fn parse_value(&mut self) -> Result<Vdf, String> {
        self.skip_ws();
        if self.i < self.s.len() && self.s[self.i] == b'{' {
            self.i += 1;
            let mut map = BTreeMap::new();
            loop {
                self.skip_ws();
                if self.i < self.s.len() && self.s[self.i] == b'}' {
                    self.i += 1;
                    break;
                }
                if self.i >= self.s.len() {
                    return Err("unclosed object".into());
                }
                let key = self.parse_string()?;
                self.skip_ws();
                let val = if self.i < self.s.len() && self.s[self.i] == b'{' {
                    self.parse_value()?
                } else {
                    Vdf::Str(self.parse_string()?)
                };
                map.insert(key, val);
            }
            return Ok(Vdf::Map(map));
        }
        // top-level "key" { ... }
        let key = self.parse_string()?;
        let val = self.parse_value()?;
        let mut map = BTreeMap::new();
        map.insert(key, val);
        Ok(Vdf::Map(map))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libraryfolders() {
        let v = parse(
            r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\SteamLib"
		"apps"
		{
			"730"		"1"
		}
	}
}
"#,
        )
        .unwrap();
        let lib = v.get("libraryfolders").unwrap().get("0").unwrap();
        assert_eq!(lib.get("path").unwrap().as_str().unwrap(), r"C:\SteamLib");
        assert!(lib.get("apps").unwrap().get("730").is_some());
    }
}
