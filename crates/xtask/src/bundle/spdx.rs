//! SPDX license expressions: legacy Cargo normalization, validation against
//! the vendored SPDX 3.27.0 identifier lists and satisfiability by the
//! license texts the bundle carries.

use std::collections::BTreeSet;

use super::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    License(String),
    With(String, String),
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
}

pub struct Identifiers {
    pub licenses: BTreeSet<String>,
    pub exceptions: BTreeSet<String>,
}

impl Identifiers {
    pub fn parse(licenses: &str, exceptions: &str) -> Self {
        let set = |text: &str| {
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect()
        };
        Self {
            licenses: set(licenses),
            exceptions: set(exceptions),
        }
    }
}

/// Cargo historically accepted `MIT/Apache-2.0` for `MIT OR Apache-2.0`.
pub fn normalize(expression: &str) -> String {
    expression
        .replace('/', " OR ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokens(expression: &str) -> Vec<String> {
    expression
        .replace('(', " ( ")
        .replace(')', " ) ")
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

struct Parser<'a> {
    tokens: Vec<String>,
    at: usize,
    identifiers: &'a Identifiers,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.at).map(String::as_str)
    }

    fn next(&mut self) -> Result<String> {
        let token = self
            .tokens
            .get(self.at)
            .cloned()
            .ok_or("unexpected end of expression")?;
        self.at += 1;
        Ok(token)
    }

    fn or(&mut self) -> Result<Expression> {
        let mut left = self.and()?;
        while self.peek() == Some("OR") {
            self.at += 1;
            left = Expression::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expression> {
        let mut left = self.primary()?;
        while self.peek() == Some("AND") {
            self.at += 1;
            left = Expression::And(Box::new(left), Box::new(self.primary()?));
        }
        Ok(left)
    }

    fn primary(&mut self) -> Result<Expression> {
        let token = self.next()?;
        if token == "(" {
            let inner = self.or()?;
            if self.next()? != ")" {
                return Err("unbalanced parenthesis".into());
            }
            return Ok(inner);
        }
        let base = token.strip_suffix('+').unwrap_or(&token);
        if !(self.identifiers.licenses.contains(base) || base.starts_with("LicenseRef-")) {
            return Err(format!("{token} is not an SPDX license identifier"));
        }
        if self.peek() == Some("WITH") {
            self.at += 1;
            let exception = self.next()?;
            if !self.identifiers.exceptions.contains(&exception) {
                return Err(format!("{exception} is not an SPDX exception identifier"));
            }
            return Ok(Expression::With(token, exception));
        }
        Ok(Expression::License(token))
    }
}

/// Parse a normalized expression, validating every identifier.
pub fn parse(expression: &str, identifiers: &Identifiers) -> Result<Expression> {
    let mut parser = Parser {
        tokens: tokens(expression),
        at: 0,
        identifiers,
    };
    let parsed = parser.or()?;
    if parser.at != parser.tokens.len() {
        return Err(format!("trailing tokens in {expression}"));
    }
    Ok(parsed)
}

/// Whether a choice of alternatives is fully covered by `available` texts.
/// An exception needs its own text as well as the license's (Deadpan's SPDX
/// directory carries none; the Deno notice set carries `LLVM-exception`).
pub fn satisfiable(expression: &Expression, available: &dyn Fn(&str) -> bool) -> bool {
    match expression {
        Expression::License(id) => available(id),
        Expression::With(id, exception) => available(id) && available(exception),
        Expression::And(left, right) => {
            satisfiable(left, available) && satisfiable(right, available)
        }
        Expression::Or(left, right) => {
            satisfiable(left, available) || satisfiable(right, available)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identifiers() -> Identifiers {
        let root = super::super::workspace_root().join("packaging/notices/spdx");
        Identifiers::parse(
            &std::fs::read_to_string(root.join("license-ids.txt")).unwrap(),
            &std::fs::read_to_string(root.join("exception-ids.txt")).unwrap(),
        )
    }

    #[test]
    fn legacy_slashes_become_or() {
        assert_eq!(normalize("MIT/Apache-2.0"), "MIT OR Apache-2.0");
        assert_eq!(normalize("Apache-2.0 / MIT"), "Apache-2.0 OR MIT");
    }

    #[test]
    fn expressions_validate_and_evaluate() {
        let ids = identifiers();
        for valid in [
            "MIT",
            "MIT OR Apache-2.0",
            "(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0",
            "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT",
            "blessing",
            "LicenseRef-cargo-example-1.0.0",
        ] {
            assert!(parse(valid, &ids).is_ok(), "{valid}");
        }
        for invalid in [
            "MIT/Apache-2.0",
            "Foo",
            "MIT OR",
            "(MIT",
            "MIT WITH Nope",
            "MIT MIT",
        ] {
            assert!(parse(invalid, &ids).is_err(), "{invalid}");
        }
        let texts = |id: &str| ["MIT", "Apache-2.0", "Zlib"].contains(&id);
        let check = |expression: &str| satisfiable(&parse(expression, &ids).unwrap(), &texts);
        assert!(check("Zlib OR Apache-2.0 OR MIT"));
        assert!(check("Apache-2.0 WITH LLVM-exception OR MIT"));
        assert!(!check("ISC"));
        assert!(!check("MIT AND BSD-2-Clause"));
        assert!(!check("Apache-2.0 WITH LLVM-exception"));
        let with_exception = |id: &str| ["Apache-2.0", "LLVM-exception"].contains(&id);
        let expression = parse("Apache-2.0 WITH LLVM-exception", &ids).unwrap();
        assert!(satisfiable(&expression, &with_exception));
    }
}
