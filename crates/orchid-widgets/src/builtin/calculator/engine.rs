//! Calculator engine — Windows/macOS-class standard + scientific evaluation.
//!
//! Standard mode uses left-to-right sequential evaluation (classic desktop
//! calculators). Scientific mode supports parentheses, operator precedence,
//! trig/log/power functions, and DEG/RAD/GRAD angle units.

#![allow(missing_docs)]

use std::fmt;

/// Display / evaluation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum CalcMode {
    #[default]
    Standard = 0,
    Scientific = 1,
}

impl CalcMode {
    #[must_use]
    pub fn from_index(i: i32) -> Self {
        match i {
            1 => Self::Scientific,
            _ => Self::Standard,
        }
    }

    #[must_use]
    pub fn as_index(self) -> i32 {
        self as i32
    }
}

/// Angle unit for trigonometric functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum AngleMode {
    #[default]
    Degrees = 0,
    Radians = 1,
    Gradians = 2,
}

impl AngleMode {
    #[must_use]
    pub fn from_index(i: i32) -> Self {
        match i {
            1 => Self::Radians,
            2 => Self::Gradians,
            _ => Self::Degrees,
        }
    }

    #[must_use]
    pub fn as_index(self) -> i32 {
        self as i32
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Degrees => "DEG",
            Self::Radians => "RAD",
            Self::Gradians => "GRAD",
        }
    }

    fn to_radians(self, x: f64) -> f64 {
        match self {
            Self::Degrees => x.to_radians(),
            Self::Radians => x,
            Self::Gradians => x * std::f64::consts::PI / 200.0,
        }
    }

    fn from_radians(self, x: f64) -> f64 {
        match self {
            Self::Degrees => x.to_degrees(),
            Self::Radians => x,
            Self::Gradians => x * 200.0 / std::f64::consts::PI,
        }
    }
}

/// One completed calculation in history.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub expression: String,
    pub result: String,
    pub value: f64,
}

/// User-facing calculator error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalcError {
    DivideByZero,
    InvalidInput,
    Overflow,
    Domain,
}

impl CalcError {
    #[must_use]
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::DivideByZero => "calc-error-divide-by-zero",
            Self::InvalidInput => "calc-error-invalid",
            Self::Overflow => "calc-error-overflow",
            Self::Domain => "calc-error-domain",
        }
    }
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.i18n_key())
    }
}

/// Button / keyboard actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalcKey {
    Digit(u8),
    Decimal,
    Backspace,
    Clear,
    ClearEntry,
    Negate,
    Percent,
    Add,
    Subtract,
    Multiply,
    Divide,
    Equals,
    Sqrt,
    Square,
    Reciprocal,
    Cube,
    CubeRoot,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Log10,
    Ln,
    Exp10,
    ExpE,
    Power,
    YRoot,
    Factorial,
    Abs,
    Mod,
    Pi,
    EConst,
    OpenParen,
    CloseParen,
    ExpNotation,
    MemClear,
    MemRecall,
    MemAdd,
    MemSub,
    MemStore,
    ToggleSecond,
    CycleAngle,
    SetMode(CalcMode),
    HistoryClear,
    HistoryRecall(usize),
}

const MAX_DIGITS: usize = 16;
const HISTORY_CAP: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    YRoot,
    Mod,
}

impl BinOp {
    fn precedence(self) -> u8 {
        match self {
            Self::Add | Self::Sub => 1,
            Self::Mul | Self::Div | Self::Mod => 2,
            Self::Pow | Self::YRoot => 3,
        }
    }

    fn right_assoc(self) -> bool {
        matches!(self, Self::Pow | Self::YRoot)
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "−",
            Self::Mul => "×",
            Self::Div => "÷",
            Self::Pow => "^",
            Self::YRoot => "ʸ√",
            Self::Mod => "mod",
        }
    }

    fn apply(self, a: f64, b: f64) -> Result<f64, CalcError> {
        let r = match self {
            Self::Add => a + b,
            Self::Sub => a - b,
            Self::Mul => a * b,
            Self::Div => {
                if b == 0.0 {
                    return Err(CalcError::DivideByZero);
                }
                a / b
            }
            Self::Pow => a.powf(b),
            Self::YRoot => {
                if b == 0.0 {
                    return Err(CalcError::DivideByZero);
                }
                a.powf(1.0 / b)
            }
            Self::Mod => {
                if b == 0.0 {
                    return Err(CalcError::DivideByZero);
                }
                a % b
            }
        };
        finite(r)
    }
}

#[derive(Debug, Clone)]
enum Frame {
    /// Value waiting for a binary op (and expression prefix for display).
    Pending {
        value: f64,
        op: BinOp,
        expr: String,
    },
    /// Open parenthesis: saved pending chain + display prefix.
    Paren {
        pending: Option<(f64, BinOp, String)>,
        expr: String,
    },
}

/// Full calculator state machine.
#[derive(Debug, Clone)]
pub struct Calculator {
    pub mode: CalcMode,
    pub angle: AngleMode,
    pub second: bool,
    /// Primary display text.
    pub display: String,
    /// Secondary expression line (e.g. "12 +").
    pub expression: String,
    /// Memory register.
    pub memory: f64,
    pub memory_set: bool,
    pub history: Vec<HistoryEntry>,
    pub error: Option<CalcError>,

    entry: String,
    value: f64,
    entering: bool,
    overwrite: bool,
    /// Last equals chain for repeated `=` (Windows-style).
    last_op: Option<(BinOp, f64)>,
    stack: Vec<Frame>,
    /// Pending binary op at current paren depth.
    pending: Option<(f64, BinOp, String)>,
}

impl Default for Calculator {
    fn default() -> Self {
        Self {
            mode: CalcMode::Standard,
            angle: AngleMode::Degrees,
            second: false,
            display: "0".into(),
            expression: String::new(),
            memory: 0.0,
            memory_set: false,
            history: Vec::new(),
            error: None,
            entry: String::new(),
            value: 0.0,
            entering: false,
            overwrite: true,
            last_op: None,
            stack: Vec::new(),
            pending: None,
        }
    }
}

impl Calculator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a key / button.
    pub fn press(&mut self, key: CalcKey) {
        if self.error.is_some()
            && !matches!(
                key,
                CalcKey::Clear
                    | CalcKey::ClearEntry
                    | CalcKey::SetMode(_)
                    | CalcKey::HistoryClear
                    | CalcKey::CycleAngle
                    | CalcKey::ToggleSecond
                    | CalcKey::Digit(_)
            )
        {
            return;
        }
        if self.error.is_some() {
            if let CalcKey::Digit(d) = key {
                self.clear_all();
                self.input_digit(d);
                self.sync_display();
                return;
            }
            if matches!(key, CalcKey::Clear | CalcKey::ClearEntry) {
                // fall through
            } else if !matches!(
                key,
                CalcKey::SetMode(_)
                    | CalcKey::HistoryClear
                    | CalcKey::CycleAngle
                    | CalcKey::ToggleSecond
            ) {
                return;
            }
        }

        match key {
            CalcKey::Digit(d) => self.input_digit(d),
            CalcKey::Decimal => self.input_decimal(),
            CalcKey::Backspace => self.backspace(),
            CalcKey::Clear => self.clear_all(),
            CalcKey::ClearEntry => self.clear_entry(),
            CalcKey::Negate => self.negate(),
            CalcKey::Percent => self.percent(),
            CalcKey::Add => self.binary(BinOp::Add),
            CalcKey::Subtract => self.binary(BinOp::Sub),
            CalcKey::Multiply => self.binary(BinOp::Mul),
            CalcKey::Divide => self.binary(BinOp::Div),
            CalcKey::Equals => self.equals(),
            CalcKey::Sqrt => self.unary(|x| {
                if x < 0.0 {
                    Err(CalcError::Domain)
                } else {
                    Ok(x.sqrt())
                }
            }),
            CalcKey::Square => self.unary(|x| Ok(x * x)),
            CalcKey::Reciprocal => self.unary(|x| {
                if x == 0.0 {
                    Err(CalcError::DivideByZero)
                } else {
                    Ok(1.0 / x)
                }
            }),
            CalcKey::Cube => self.unary(|x| Ok(x * x * x)),
            CalcKey::CubeRoot => self.unary(|x| Ok(x.cbrt())),
            CalcKey::Sin => {
                let angle = self.angle;
                if self.second {
                    self.second = false;
                    self.unary(|x| finite(angle.from_radians(x.asin())));
                } else {
                    self.unary(|x| finite(angle.to_radians(x).sin()));
                }
            }
            CalcKey::Cos => {
                let angle = self.angle;
                if self.second {
                    self.second = false;
                    self.unary(|x| finite(angle.from_radians(x.acos())));
                } else {
                    self.unary(|x| finite(angle.to_radians(x).cos()));
                }
            }
            CalcKey::Tan => {
                let angle = self.angle;
                if self.second {
                    self.second = false;
                    self.unary(|x| finite(angle.from_radians(x.atan())));
                } else {
                    self.unary(|x| finite(angle.to_radians(x).tan()));
                }
            }
            CalcKey::Asin => {
                let angle = self.angle;
                self.unary(|x| finite(angle.from_radians(x.asin())));
            }
            CalcKey::Acos => {
                let angle = self.angle;
                self.unary(|x| finite(angle.from_radians(x.acos())));
            }
            CalcKey::Atan => {
                let angle = self.angle;
                self.unary(|x| finite(angle.from_radians(x.atan())));
            }
            CalcKey::Sinh => self.unary(|x| finite(x.sinh())),
            CalcKey::Cosh => self.unary(|x| finite(x.cosh())),
            CalcKey::Tanh => self.unary(|x| finite(x.tanh())),
            CalcKey::Log10 => {
                if self.second {
                    self.second = false;
                    self.unary(|x| finite(10f64.powf(x)));
                } else {
                    self.unary(|x| {
                        if x <= 0.0 {
                            Err(CalcError::Domain)
                        } else {
                            Ok(x.log10())
                        }
                    });
                }
            }
            CalcKey::Ln => {
                if self.second {
                    self.second = false;
                    self.unary(|x| finite(x.exp()));
                } else {
                    self.unary(|x| {
                        if x <= 0.0 {
                            Err(CalcError::Domain)
                        } else {
                            Ok(x.ln())
                        }
                    });
                }
            }
            CalcKey::Exp10 => self.unary(|x| finite(10f64.powf(x))),
            CalcKey::ExpE => self.unary(|x| finite(x.exp())),
            CalcKey::Power => self.binary(BinOp::Pow),
            CalcKey::YRoot => self.binary(BinOp::YRoot),
            CalcKey::Factorial => self.unary(factorial),
            CalcKey::Abs => self.unary(|x| Ok(x.abs())),
            CalcKey::Mod => self.binary(BinOp::Mod),
            CalcKey::Pi => self.insert_constant(std::f64::consts::PI),
            CalcKey::EConst => self.insert_constant(std::f64::consts::E),
            CalcKey::OpenParen => self.open_paren(),
            CalcKey::CloseParen => self.close_paren(),
            CalcKey::ExpNotation => self.exp_notation(),
            CalcKey::MemClear => {
                self.memory = 0.0;
                self.memory_set = false;
            }
            CalcKey::MemRecall => {
                if self.memory_set {
                    self.set_value(self.memory);
                    self.overwrite = true;
                    self.entering = false;
                    self.last_op = None;
                }
            }
            CalcKey::MemAdd => {
                if let Ok(v) = self.current_value() {
                    self.memory += v;
                    self.memory_set = true;
                    self.overwrite = true;
                }
            }
            CalcKey::MemSub => {
                if let Ok(v) = self.current_value() {
                    self.memory -= v;
                    self.memory_set = true;
                    self.overwrite = true;
                }
            }
            CalcKey::MemStore => {
                if let Ok(v) = self.current_value() {
                    self.memory = v;
                    self.memory_set = true;
                    self.overwrite = true;
                }
            }
            CalcKey::ToggleSecond => {
                self.second = !self.second;
            }
            CalcKey::CycleAngle => {
                self.angle = match self.angle {
                    AngleMode::Degrees => AngleMode::Radians,
                    AngleMode::Radians => AngleMode::Gradians,
                    AngleMode::Gradians => AngleMode::Degrees,
                };
            }
            CalcKey::SetMode(m) => {
                self.mode = m;
                self.second = false;
                if m == CalcMode::Standard {
                    while let Some(Frame::Paren { pending, expr }) = self.stack.pop() {
                        self.pending = pending;
                        self.expression = expr;
                    }
                }
            }
            CalcKey::HistoryClear => self.history.clear(),
            CalcKey::HistoryRecall(i) => {
                if let Some(h) = self.history.get(i).cloned() {
                    self.set_value(h.value);
                    self.expression.clear();
                    self.pending = None;
                    self.stack.clear();
                    self.last_op = None;
                    self.overwrite = true;
                    self.entering = false;
                }
            }
        }
        self.sync_display();
    }

    /// Parse a keyboard character into a key.
    #[must_use]
    pub fn key_from_text(text: &str, ctrl: bool, _shift: bool) -> Option<CalcKey> {
        if ctrl {
            return None;
        }
        match text {
            "0" => Some(CalcKey::Digit(0)),
            "1" => Some(CalcKey::Digit(1)),
            "2" => Some(CalcKey::Digit(2)),
            "3" => Some(CalcKey::Digit(3)),
            "4" => Some(CalcKey::Digit(4)),
            "5" => Some(CalcKey::Digit(5)),
            "6" => Some(CalcKey::Digit(6)),
            "7" => Some(CalcKey::Digit(7)),
            "8" => Some(CalcKey::Digit(8)),
            "9" => Some(CalcKey::Digit(9)),
            "." | "," => Some(CalcKey::Decimal),
            "+" => Some(CalcKey::Add),
            "-" => Some(CalcKey::Subtract),
            "*" => Some(CalcKey::Multiply),
            "/" => Some(CalcKey::Divide),
            "=" | "\n" | "\r" => Some(CalcKey::Equals),
            "%" => Some(CalcKey::Percent),
            "(" => Some(CalcKey::OpenParen),
            ")" => Some(CalcKey::CloseParen),
            "^" => Some(CalcKey::Power),
            "!" => Some(CalcKey::Factorial),
            "\u{8}" => Some(CalcKey::Backspace),
            "\u{7f}" => Some(CalcKey::ClearEntry),
            "\u{1b}" => Some(CalcKey::Clear),
            _ => None,
        }
    }

    /// Map UI button id strings to keys (stable contract with Slint).
    #[must_use]
    pub fn key_from_id(id: &str) -> Option<CalcKey> {
        Some(match id {
            "0" => CalcKey::Digit(0),
            "1" => CalcKey::Digit(1),
            "2" => CalcKey::Digit(2),
            "3" => CalcKey::Digit(3),
            "4" => CalcKey::Digit(4),
            "5" => CalcKey::Digit(5),
            "6" => CalcKey::Digit(6),
            "7" => CalcKey::Digit(7),
            "8" => CalcKey::Digit(8),
            "9" => CalcKey::Digit(9),
            "dec" => CalcKey::Decimal,
            "bs" => CalcKey::Backspace,
            "c" => CalcKey::Clear,
            "ce" => CalcKey::ClearEntry,
            "neg" => CalcKey::Negate,
            "pct" => CalcKey::Percent,
            "add" => CalcKey::Add,
            "sub" => CalcKey::Subtract,
            "mul" => CalcKey::Multiply,
            "div" => CalcKey::Divide,
            "eq" => CalcKey::Equals,
            "sqrt" => CalcKey::Sqrt,
            "sqr" => CalcKey::Square,
            "inv" => CalcKey::Reciprocal,
            "cube" => CalcKey::Cube,
            "cbrt" => CalcKey::CubeRoot,
            "sin" => CalcKey::Sin,
            "cos" => CalcKey::Cos,
            "tan" => CalcKey::Tan,
            "asin" => CalcKey::Asin,
            "acos" => CalcKey::Acos,
            "atan" => CalcKey::Atan,
            "sinh" => CalcKey::Sinh,
            "cosh" => CalcKey::Cosh,
            "tanh" => CalcKey::Tanh,
            "log" => CalcKey::Log10,
            "ln" => CalcKey::Ln,
            "exp10" => CalcKey::Exp10,
            "expe" => CalcKey::ExpE,
            "pow" => CalcKey::Power,
            "yroot" => CalcKey::YRoot,
            "fact" => CalcKey::Factorial,
            "abs" => CalcKey::Abs,
            "mod" => CalcKey::Mod,
            "pi" => CalcKey::Pi,
            "e" => CalcKey::EConst,
            "(" => CalcKey::OpenParen,
            ")" => CalcKey::CloseParen,
            "exp" => CalcKey::ExpNotation,
            "mc" => CalcKey::MemClear,
            "mr" => CalcKey::MemRecall,
            "mplus" => CalcKey::MemAdd,
            "mminus" => CalcKey::MemSub,
            "ms" => CalcKey::MemStore,
            "2nd" => CalcKey::ToggleSecond,
            "angle" => CalcKey::CycleAngle,
            "mode-std" => CalcKey::SetMode(CalcMode::Standard),
            "mode-sci" => CalcKey::SetMode(CalcMode::Scientific),
            "hist-clear" => CalcKey::HistoryClear,
            _ => return None,
        })
    }

    fn input_digit(&mut self, d: u8) {
        self.last_op = None;
        if self.overwrite || !self.entering {
            self.entry.clear();
            self.entering = true;
            self.overwrite = false;
        }
        if self.entry == "0" {
            self.entry.clear();
        }
        if significant_digit_count(&self.entry) >= MAX_DIGITS {
            return;
        }
        self.entry.push(char::from(b'0' + d));
        self.value = parse_entry(&self.entry).unwrap_or(0.0);
    }

    fn input_decimal(&mut self) {
        self.last_op = None;
        if self.overwrite || !self.entering {
            self.entry = "0".into();
            self.entering = true;
            self.overwrite = false;
        }
        if self.entry.contains('.') || self.entry.contains('e') || self.entry.contains('E') {
            return;
        }
        if self.entry.is_empty() {
            self.entry.push('0');
        }
        self.entry.push('.');
    }

    fn backspace(&mut self) {
        if self.error.is_some() {
            self.clear_all();
            return;
        }
        if !self.entering || self.overwrite {
            return;
        }
        self.entry.pop();
        if self.entry.is_empty() || self.entry == "-" {
            self.entry.clear();
            self.value = 0.0;
            self.entering = false;
            self.overwrite = true;
        } else {
            self.value = parse_entry(&self.entry).unwrap_or(0.0);
        }
    }

    fn clear_all(&mut self) {
        let mode = self.mode;
        let angle = self.angle;
        let memory = self.memory;
        let memory_set = self.memory_set;
        let history = std::mem::take(&mut self.history);
        *self = Self::default();
        self.mode = mode;
        self.angle = angle;
        self.memory = memory;
        self.memory_set = memory_set;
        self.history = history;
    }

    fn clear_entry(&mut self) {
        self.error = None;
        self.entry.clear();
        self.value = 0.0;
        self.entering = false;
        self.overwrite = true;
        self.last_op = None;
    }

    fn negate(&mut self) {
        if self.entering && !self.overwrite {
            if let Some(rest) = self.entry.strip_prefix('-') {
                self.entry = rest.to_string();
            } else if !self.entry.is_empty() {
                self.entry.insert(0, '-');
            }
            self.value = parse_entry(&self.entry).unwrap_or(0.0);
        } else {
            self.value = -self.value;
            self.entry.clear();
            self.entering = false;
            self.overwrite = true;
        }
    }

    fn percent(&mut self) {
        let Ok(current) = self.current_value() else {
            return;
        };
        let result = if let Some((base, op, _)) = self.pending {
            match op {
                BinOp::Add | BinOp::Sub => base * current / 100.0,
                _ => current / 100.0,
            }
        } else {
            current / 100.0
        };
        match finite(result) {
            Ok(v) => {
                self.set_value(v);
                self.overwrite = true;
                self.entering = false;
            }
            Err(e) => self.set_error(e),
        }
    }

    fn binary(&mut self, op: BinOp) {
        let Ok(current) = self.current_value() else {
            return;
        };
        self.last_op = None;

        if let Some((left, prev, expr)) = self.pending.take() {
            let should_reduce = match self.mode {
                CalcMode::Standard => true,
                CalcMode::Scientific => {
                    prev.precedence() > op.precedence()
                        || (prev.precedence() == op.precedence() && !op.right_assoc())
                }
            };
            if should_reduce {
                match prev.apply(left, current) {
                    Ok(v) => {
                        let new_expr =
                            format!("{expr} {} {}", prev.symbol(), format_number(current));
                        self.value = v;
                        self.pending = Some((v, op, new_expr.clone()));
                        self.expression = format!("{new_expr} {}", op.symbol());
                    }
                    Err(e) => {
                        self.set_error(e);
                        return;
                    }
                }
            } else {
                self.stack.push(Frame::Pending {
                    value: left,
                    op: prev,
                    expr,
                });
                let cur = format_number(current);
                self.pending = Some((current, op, cur.clone()));
                self.expression = format!("{cur} {}", op.symbol());
            }
        } else {
            let cur = format_number(current);
            self.pending = Some((current, op, cur.clone()));
            self.expression = format!("{cur} {}", op.symbol());
        }

        self.entering = false;
        self.overwrite = true;
        self.entry.clear();
    }

    fn equals(&mut self) {
        if self.pending.is_none() && self.stack.is_empty() {
            if let Some((op, rhs)) = self.last_op {
                let Ok(lhs) = self.current_value() else {
                    return;
                };
                match op.apply(lhs, rhs) {
                    Ok(v) => {
                        let expr = format!(
                            "{} {} {}",
                            format_number(lhs),
                            op.symbol(),
                            format_number(rhs)
                        );
                        self.push_history(&expr, v);
                        self.set_value(v);
                        self.expression.clear();
                        self.overwrite = true;
                        self.entering = false;
                    }
                    Err(e) => self.set_error(e),
                }
            }
            return;
        }

        let Ok(mut current) = self.current_value() else {
            return;
        };

        while self.stack.iter().any(|f| matches!(f, Frame::Paren { .. })) {
            if let Err(e) = self.close_paren_inner(&mut current) {
                self.set_error(e);
                return;
            }
        }

        let mut expr_full = String::new();
        let mut last_rhs = None;
        let mut last_binop = None;

        // Reduce precedence stack + pending from innermost.
        loop {
            if let Some((left, op, expr)) = self.pending.take() {
                expr_full = format!("{expr} {} {}", op.symbol(), format_number(current));
                match op.apply(left, current) {
                    Ok(v) => {
                        last_binop = Some(op);
                        last_rhs = Some(current);
                        current = v;
                    }
                    Err(e) => {
                        self.set_error(e);
                        return;
                    }
                }
                continue;
            }
            match self.stack.pop() {
                Some(Frame::Pending { value, op, expr }) => {
                    expr_full = format!("{expr} {} {}", op.symbol(), format_number(current));
                    match op.apply(value, current) {
                        Ok(v) => {
                            last_binop = Some(op);
                            last_rhs = Some(current);
                            current = v;
                        }
                        Err(e) => {
                            self.set_error(e);
                            return;
                        }
                    }
                }
                Some(Frame::Paren { pending, expr }) => {
                    self.pending = pending;
                    let _ = expr;
                }
                None => break,
            }
        }

        if let (Some(op), Some(rhs)) = (last_binop, last_rhs) {
            self.last_op = Some((op, rhs));
        }

        if expr_full.is_empty() {
            expr_full = format_number(current);
        }
        self.push_history(&expr_full, current);
        self.set_value(current);
        self.expression.clear();
        self.overwrite = true;
        self.entering = false;
        self.entry.clear();
    }

    fn unary(&mut self, f: impl FnOnce(f64) -> Result<f64, CalcError>) {
        let Ok(x) = self.current_value() else {
            return;
        };
        match f(x).and_then(finite) {
            Ok(v) => {
                self.set_value(v);
                self.overwrite = true;
                self.entering = false;
                self.last_op = None;
            }
            Err(e) => self.set_error(e),
        }
    }

    fn insert_constant(&mut self, v: f64) {
        self.set_value(v);
        self.overwrite = true;
        self.entering = false;
        self.last_op = None;
    }

    fn open_paren(&mut self) {
        if self.mode == CalcMode::Standard {
            return;
        }
        let expr = if self.expression.is_empty() {
            "(".into()
        } else {
            format!("{} (", self.expression.trim_end())
        };
        self.stack.push(Frame::Paren {
            pending: self.pending.take(),
            expr: expr.clone(),
        });
        self.expression = expr;
        self.entering = false;
        self.overwrite = true;
        self.entry.clear();
        self.value = 0.0;
        self.last_op = None;
    }

    fn close_paren(&mut self) {
        if self.mode == CalcMode::Standard {
            return;
        }
        let Ok(mut current) = self.current_value() else {
            return;
        };
        if let Err(e) = self.close_paren_inner(&mut current) {
            self.set_error(e);
            return;
        }
        self.set_value(current);
        self.entering = false;
        self.overwrite = true;
        self.entry.clear();
        self.last_op = None;
    }

    fn close_paren_inner(&mut self, current: &mut f64) -> Result<(), CalcError> {
        if let Some((left, op, expr)) = self.pending.take() {
            *current = op.apply(left, *current)?;
            self.expression = format!("{expr} {} {}", op.symbol(), format_number(*current));
        }

        while let Some(top) = self.stack.last() {
            if matches!(top, Frame::Paren { .. }) {
                break;
            }
            if let Some(Frame::Pending { value, op, .. }) = self.stack.pop() {
                *current = op.apply(value, *current)?;
            }
        }

        match self.stack.pop() {
            Some(Frame::Paren { pending, expr }) => {
                self.pending = pending;
                self.expression = format!("{expr}{})", format_number(*current));
                Ok(())
            }
            Some(other) => {
                self.stack.push(other);
                Err(CalcError::InvalidInput)
            }
            None => Err(CalcError::InvalidInput),
        }
    }

    fn exp_notation(&mut self) {
        if self.overwrite || !self.entering {
            self.entry = "1".into();
            self.entering = true;
            self.overwrite = false;
        }
        if self.entry.contains('e') || self.entry.contains('E') {
            return;
        }
        self.entry.push('e');
    }

    fn current_value(&self) -> Result<f64, CalcError> {
        if let Some(err) = self.error {
            return Err(err);
        }
        if self.entering && !self.entry.is_empty() {
            parse_entry(&self.entry).ok_or(CalcError::InvalidInput)
        } else {
            finite(self.value)
        }
    }

    fn set_value(&mut self, v: f64) {
        match finite(v) {
            Ok(v) => {
                self.error = None;
                self.value = v;
                self.entry.clear();
                self.display = format_number(v);
            }
            Err(e) => self.set_error(e),
        }
    }

    fn set_error(&mut self, e: CalcError) {
        self.error = Some(e);
        self.display = e.i18n_key().into();
        self.expression.clear();
        self.pending = None;
        self.stack.clear();
        self.last_op = None;
        self.entering = false;
        self.overwrite = true;
        self.entry.clear();
    }

    fn push_history(&mut self, expression: &str, value: f64) {
        let result = format_number(value);
        self.history.insert(
            0,
            HistoryEntry {
                expression: expression.to_string(),
                result,
                value,
            },
        );
        if self.history.len() > HISTORY_CAP {
            self.history.truncate(HISTORY_CAP);
        }
    }

    fn sync_display(&mut self) {
        if self.error.is_some() {
            return;
        }
        if self.entering && !self.entry.is_empty() {
            self.display = self.entry.clone();
        } else {
            self.display = format_number(self.value);
        }
    }
}

fn finite(v: f64) -> Result<f64, CalcError> {
    if v.is_nan() {
        Err(CalcError::Domain)
    } else if v.is_infinite() {
        Err(CalcError::Overflow)
    } else {
        Ok(v)
    }
}

fn parse_entry(s: &str) -> Option<f64> {
    if s.is_empty() || s == "-" || s == "." || s == "-." {
        return Some(0.0);
    }
    if s.ends_with('e') || s.ends_with('E') {
        return s[..s.len() - 1].parse().ok().or(Some(0.0));
    }
    if s.ends_with("e-")
        || s.ends_with("E-")
        || s.ends_with("e+")
        || s.ends_with("E+")
    {
        return s[..s.len() - 2].parse().ok().or(Some(0.0));
    }
    s.parse().ok()
}

fn significant_digit_count(s: &str) -> usize {
    let body = s.split(['e', 'E']).next().unwrap_or(s);
    body.chars().filter(|c| c.is_ascii_digit()).count()
}

fn factorial(x: f64) -> Result<f64, CalcError> {
    if x < 0.0 || x.fract() != 0.0 {
        return Err(CalcError::Domain);
    }
    if x > 170.0 {
        return Err(CalcError::Overflow);
    }
    let n = x as u32;
    let mut acc = 1.0f64;
    for i in 2..=n {
        acc *= f64::from(i);
    }
    finite(acc)
}

/// Format a number for calculator display (up to 16 significant digits).
#[must_use]
pub fn format_number(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let abs = v.abs();
    if !(1e-4..1e16).contains(&abs) {
        let s = format!("{v:.15e}");
        return trim_sci(&s);
    }
    let s = format!("{v:.15}");
    trim_fixed(&s)
}

fn trim_fixed(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let t = s.trim_end_matches('0');
    let t = t.trim_end_matches('.');
    if t.is_empty() || t == "-" {
        "0".into()
    } else {
        t.to_string()
    }
}

fn trim_sci(s: &str) -> String {
    let Some((mant, exp)) = s.split_once('e') else {
        return s.to_string();
    };
    let mant = trim_fixed(mant);
    format!("{mant}e{exp}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calc() -> Calculator {
        Calculator::new()
    }

    fn press_seq(c: &mut Calculator, keys: &[CalcKey]) {
        for k in keys {
            c.press(*k);
        }
    }

    #[test]
    fn basic_addition() {
        let mut c = calc();
        press_seq(
            &mut c,
            &[
                CalcKey::Digit(2),
                CalcKey::Add,
                CalcKey::Digit(3),
                CalcKey::Equals,
            ],
        );
        assert_eq!(c.display, "5");
    }

    #[test]
    fn sequential_standard_no_precedence() {
        let mut c = calc();
        c.mode = CalcMode::Standard;
        press_seq(
            &mut c,
            &[
                CalcKey::Digit(2),
                CalcKey::Add,
                CalcKey::Digit(3),
                CalcKey::Multiply,
                CalcKey::Digit(4),
                CalcKey::Equals,
            ],
        );
        assert_eq!(c.display, "20");
    }

    #[test]
    fn scientific_precedence() {
        let mut c = calc();
        c.mode = CalcMode::Scientific;
        press_seq(
            &mut c,
            &[
                CalcKey::Digit(2),
                CalcKey::Add,
                CalcKey::Digit(3),
                CalcKey::Multiply,
                CalcKey::Digit(4),
                CalcKey::Equals,
            ],
        );
        assert_eq!(c.display, "14");
    }

    #[test]
    fn divide_by_zero() {
        let mut c = calc();
        press_seq(
            &mut c,
            &[
                CalcKey::Digit(1),
                CalcKey::Divide,
                CalcKey::Digit(0),
                CalcKey::Equals,
            ],
        );
        assert_eq!(c.error, Some(CalcError::DivideByZero));
    }

    #[test]
    fn percent_of_base() {
        let mut c = calc();
        press_seq(
            &mut c,
            &[
                CalcKey::Digit(5),
                CalcKey::Digit(0),
                CalcKey::Add,
                CalcKey::Digit(1),
                CalcKey::Digit(0),
                CalcKey::Percent,
                CalcKey::Equals,
            ],
        );
        assert_eq!(c.display, "55");
    }

    #[test]
    fn sqrt_and_square() {
        let mut c = calc();
        press_seq(&mut c, &[CalcKey::Digit(9), CalcKey::Sqrt]);
        assert_eq!(c.display, "3");
        press_seq(&mut c, &[CalcKey::Square]);
        assert_eq!(c.display, "9");
    }

    #[test]
    fn memory_roundtrip() {
        let mut c = calc();
        press_seq(
            &mut c,
            &[CalcKey::Digit(4), CalcKey::Digit(2), CalcKey::MemStore],
        );
        assert!(c.memory_set);
        press_seq(&mut c, &[CalcKey::Clear, CalcKey::MemRecall]);
        assert_eq!(c.display, "42");
        press_seq(&mut c, &[CalcKey::MemAdd]);
        press_seq(&mut c, &[CalcKey::Clear, CalcKey::MemRecall]);
        assert_eq!(c.display, "84");
    }

    #[test]
    fn sin_90_degrees() {
        let mut c = calc();
        c.mode = CalcMode::Scientific;
        c.angle = AngleMode::Degrees;
        press_seq(
            &mut c,
            &[CalcKey::Digit(9), CalcKey::Digit(0), CalcKey::Sin],
        );
        let v: f64 = c.display.parse().unwrap();
        assert!((v - 1.0).abs() < 1e-12);
    }

    #[test]
    fn factorial() {
        let mut c = calc();
        press_seq(&mut c, &[CalcKey::Digit(5), CalcKey::Factorial]);
        assert_eq!(c.display, "120");
    }

    #[test]
    fn parentheses() {
        let mut c = calc();
        c.mode = CalcMode::Scientific;
        press_seq(
            &mut c,
            &[
                CalcKey::OpenParen,
                CalcKey::Digit(2),
                CalcKey::Add,
                CalcKey::Digit(3),
                CalcKey::CloseParen,
                CalcKey::Multiply,
                CalcKey::Digit(4),
                CalcKey::Equals,
            ],
        );
        assert_eq!(c.display, "20");
    }

    #[test]
    fn repeated_equals() {
        let mut c = calc();
        press_seq(
            &mut c,
            &[
                CalcKey::Digit(5),
                CalcKey::Add,
                CalcKey::Digit(3),
                CalcKey::Equals,
            ],
        );
        assert_eq!(c.display, "8");
        c.press(CalcKey::Equals);
        assert_eq!(c.display, "11");
        c.press(CalcKey::Equals);
        assert_eq!(c.display, "14");
    }

    #[test]
    fn history_recorded() {
        let mut c = calc();
        press_seq(
            &mut c,
            &[
                CalcKey::Digit(1),
                CalcKey::Add,
                CalcKey::Digit(1),
                CalcKey::Equals,
            ],
        );
        assert_eq!(c.history.len(), 1);
        assert_eq!(c.history[0].result, "2");
    }

    #[test]
    fn clear_resets_error() {
        let mut c = calc();
        press_seq(
            &mut c,
            &[
                CalcKey::Digit(1),
                CalcKey::Divide,
                CalcKey::Digit(0),
                CalcKey::Equals,
            ],
        );
        assert!(c.error.is_some());
        c.press(CalcKey::Clear);
        assert!(c.error.is_none());
        assert_eq!(c.display, "0");
    }

    #[test]
    fn format_trims_zeros() {
        assert_eq!(format_number(2.5), "2.5");
        assert_eq!(format_number(3.0), "3");
        assert_eq!(format_number(0.0), "0");
    }

    #[test]
    fn key_from_id_covers_core() {
        assert_eq!(Calculator::key_from_id("add"), Some(CalcKey::Add));
        assert_eq!(Calculator::key_from_id("sin"), Some(CalcKey::Sin));
        assert_eq!(Calculator::key_from_id("nope"), None);
    }
}
