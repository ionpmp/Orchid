//! Free-form expression evaluation for quick search (`=2+2`).

use super::engine::{format_number, AngleMode, CalcError};

/// Evaluate a scientific-style expression string.
pub fn evaluate_expression(input: &str, angle: AngleMode) -> Result<f64, CalcError> {
    let mut p = Parser::new(input, angle);
    let v = p.parse_expr()?;
    p.skip_ws();
    if p.peek().is_some() {
        return Err(CalcError::InvalidInput);
    }
    finite(v)
}

/// Format a successful quick-calc result for UI display.
#[must_use]
pub fn format_result(v: f64) -> String {
    format_number(v)
}

struct Parser<'a> {
    src: &'a [u8],
    i: usize,
    angle: AngleMode,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str, angle: AngleMode) -> Self {
        Self { src: s.as_bytes(), i: 0, angle }
    }
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) { self.i += 1; }
    }
    fn peek(&self) -> Option<u8> { self.src.get(self.i).copied() }
    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.i += 1;
        Some(c)
    }
    fn parse_expr(&mut self) -> Result<f64, CalcError> { self.parse_add() }
    fn parse_add(&mut self) -> Result<f64, CalcError> {
        let mut left = self.parse_mul()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'+') => { self.bump(); left = finite(left + self.parse_mul()?)?; }
                Some(b'-') => { self.bump(); left = finite(left - self.parse_mul()?)?; }
                _ => break,
            }
        }
        Ok(left)
    }
    fn parse_mul(&mut self) -> Result<f64, CalcError> {
        let mut left = self.parse_power()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'*') => { self.bump(); left = finite(left * self.parse_power()?)?; }
                Some(b'/') => {
                    self.bump();
                    let rhs = self.parse_power()?;
                    if rhs == 0.0 { return Err(CalcError::DivideByZero); }
                    left = finite(left / rhs)?;
                }
                Some(b'%') => {
                    self.bump();
                    let rhs = self.parse_power()?;
                    if rhs == 0.0 { return Err(CalcError::DivideByZero); }
                    left = finite(left % rhs)?;
                }
                _ => break,
            }
        }
        Ok(left)
    }
    fn parse_power(&mut self) -> Result<f64, CalcError> {
        let base = self.parse_unary()?;
        self.skip_ws();
        if self.peek() == Some(b'^') {
            self.bump();
            let exp = self.parse_power()?;
            finite(base.powf(exp))
        } else { Ok(base) }
    }
    fn parse_unary(&mut self) -> Result<f64, CalcError> {
        self.skip_ws();
        match self.peek() {
            Some(b'+') => { self.bump(); self.parse_unary() }
            Some(b'-') => { self.bump(); Ok(-self.parse_unary()?) }
            _ => self.parse_primary(),
        }
    }
    fn parse_primary(&mut self) -> Result<f64, CalcError> {
        self.skip_ws();
        match self.peek() {
            Some(b'(') => {
                self.bump();
                let v = self.parse_expr()?;
                self.skip_ws();
                if self.bump() != Some(b')') { return Err(CalcError::InvalidInput); }
                Ok(v)
            }
            Some(b'0'..=b'9') | Some(b'.') => self.parse_number(),
            Some(b'a'..=b'z') | Some(b'A'..=b'Z') | Some(b'_') => self.parse_ident_or_call(),
            Some(0xCF) => self.parse_pi_symbol(),
            _ => Err(CalcError::InvalidInput),
        }
    }
    fn parse_pi_symbol(&mut self) -> Result<f64, CalcError> {
        if self.src.get(self.i..self.i + 2) == Some(&[0xCF, 0x80]) {
            self.i += 2;
            return Ok(std::f64::consts::PI);
        }
        Err(CalcError::InvalidInput)
    }
    fn parse_number(&mut self) -> Result<f64, CalcError> {
        let start = self.i;
        while matches!(self.peek(), Some(b'0'..=b'9')) { self.bump(); }
        if self.peek() == Some(b'.') {
            self.bump();
            while matches!(self.peek(), Some(b'0'..=b'9')) { self.bump(); }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let save = self.i;
            self.bump();
            if matches!(self.peek(), Some(b'+' | b'-')) { self.bump(); }
            if matches!(self.peek(), Some(b'0'..=b'9')) {
                while matches!(self.peek(), Some(b'0'..=b'9')) { self.bump(); }
            } else { self.i = save; }
        }
        let s = std::str::from_utf8(&self.src[start..self.i]).map_err(|_| CalcError::InvalidInput)?;
        s.parse::<f64>().map_err(|_| CalcError::InvalidInput).and_then(finite)
    }
    fn parse_ident_or_call(&mut self) -> Result<f64, CalcError> {
        let start = self.i;
        while matches!(self.peek(), Some(b'a'..=b'z') | Some(b'A'..=b'Z') | Some(b'0'..=b'9') | Some(b'_')) {
            self.bump();
        }
        let name = std::str::from_utf8(&self.src[start..self.i])
            .map_err(|_| CalcError::InvalidInput)?
            .to_ascii_lowercase();
        self.skip_ws();
        if self.peek() == Some(b'(') {
            self.bump();
            let arg = self.parse_expr()?;
            self.skip_ws();
            if self.bump() != Some(b')') { return Err(CalcError::InvalidInput); }
            return apply_fn(&name, arg, self.angle);
        }
        match name.as_str() {
            "pi" => Ok(std::f64::consts::PI),
            "e" => Ok(std::f64::consts::E),
            _ => Err(CalcError::InvalidInput),
        }
    }
}

fn apply_fn(name: &str, x: f64, angle: AngleMode) -> Result<f64, CalcError> {
    let r = match name {
        "sin" => angle.to_radians(x).sin(),
        "cos" => angle.to_radians(x).cos(),
        "tan" => angle.to_radians(x).tan(),
        "asin" => angle.from_radians(x.asin()),
        "acos" => angle.from_radians(x.acos()),
        "atan" => angle.from_radians(x.atan()),
        "sinh" => x.sinh(),
        "cosh" => x.cosh(),
        "tanh" => x.tanh(),
        "log" | "log10" => { if x <= 0.0 { return Err(CalcError::Domain); } x.log10() }
        "ln" | "log_e" => { if x <= 0.0 { return Err(CalcError::Domain); } x.ln() }
        "sqrt" => { if x < 0.0 { return Err(CalcError::Domain); } x.sqrt() }
        "cbrt" => x.cbrt(),
        "abs" => x.abs(),
        "exp" => x.exp(),
        _ => return Err(CalcError::InvalidInput),
    };
    finite(r)
}

fn finite(v: f64) -> Result<f64, CalcError> {
    if v.is_nan() { Err(CalcError::Domain) }
    else if v.is_infinite() { Err(CalcError::Overflow) }
    else { Ok(v) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic_ops() {
        assert_eq!(evaluate_expression("2+3*4", AngleMode::Degrees).unwrap(), 14.0);
        assert_eq!(evaluate_expression("(2+3)*4", AngleMode::Degrees).unwrap(), 20.0);
    }
    #[test]
    fn functions_and_constants() {
        let v = evaluate_expression("sin(90)", AngleMode::Degrees).unwrap();
        assert!((v - 1.0).abs() < 1e-12);
    }
    #[test]
    fn divide_by_zero() {
        assert_eq!(evaluate_expression("1/0", AngleMode::Degrees), Err(CalcError::DivideByZero));
    }
}
