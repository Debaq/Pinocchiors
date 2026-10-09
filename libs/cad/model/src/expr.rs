//! Expresiones de parámetros: `ancho / 2 + 3`, `max(alto, 10)`, `2*pi*r`.
//!
//! Números (con punto o coma decimal), `+ - * / ^`, paréntesis, menos unario,
//! constantes `pi` y `e`, funciones `sin cos tan asin acos atan` (en grados),
//! `sqrt abs min max round floor ceil` y nombres de parámetros. Un número puede
//! llevar unidad: `mm cm m in " ft` (pasa a mm) o `deg ° rad` (pasa a grados).

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(char),
    LParen,
    RParen,
    Comma,
}

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            // Coma decimal: "2,5" fuera de una llamada; dentro, "max(1,2)" separa.
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || (chars[i] == ',' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() && !in_call(&out))) {
                i += 1;
            }
            // Exponente 1e-3
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') && i + 1 < chars.len() && (chars[i + 1].is_ascii_digit() || chars[i + 1] == '-' || chars[i + 1] == '+') {
                i += 2;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let text: String = chars[start..i].iter().collect::<String>().replace(',', ".");
            let mut value: f64 = text.parse().map_err(|_| format!("número inválido: {text}"))?;
            // Unidad pegada al número: se pasa a mm o a grados
            let rest = &src[char_offset(src, i)..];
            let trimmed = rest.trim_start();
            for (unit, k) in UNITS {
                let after = trimmed.strip_prefix(unit);
                if after.is_some_and(|a| !a.starts_with(|c: char| c.is_alphanumeric() || c == '_')) {
                    value *= k;
                    i += rest[..rest.len() - trimmed.len() + unit.len()].chars().count();
                    break;
                }
            }
            out.push(Tok::Num(value));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Tok::Ident(chars[start..i].iter().collect()));
        } else {
            out.push(match c {
                '+' | '-' | '*' | '/' | '^' => Tok::Op(c),
                '(' => Tok::LParen,
                ')' => Tok::RParen,
                ',' | ';' => Tok::Comma,
                _ => return Err(format!("carácter no válido: «{c}»")),
            });
            i += 1;
        }
    }
    Ok(out)
}

/// Unidades que se pueden escribir después de un número (largos en mm,
/// ángulos en grados). "mm" antes que "m".
const UNITS: [(&str, f64); 9] = [
    ("mm", 1.0),
    ("cm", 10.0),
    ("m", 1000.0),
    ("in", 25.4),
    ("\"", 25.4),
    ("ft", 304.8),
    ("deg", 1.0),
    ("°", 1.0),
    ("rad", 180.0 / std::f64::consts::PI),
];

/// Dentro de los argumentos de una función la coma separa, no es decimal.
fn in_call(toks: &[Tok]) -> bool {
    let mut depth = 0i32;
    for t in toks.iter().rev() {
        match t {
            Tok::RParen => depth += 1,
            Tok::LParen if depth == 0 => return true,
            Tok::LParen => depth -= 1,
            _ => {}
        }
    }
    false
}

fn char_offset(s: &str, chars: usize) -> usize {
    s.char_indices().nth(chars).map_or(s.len(), |(i, _)| i)
}

#[derive(Debug, Clone)]
enum Node {
    Num(f64),
    Var(String),
    Neg(Box<Node>),
    Bin(char, Box<Node>, Box<Node>),
    Call(String, Vec<Node>),
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn expr(&mut self) -> Result<Node, String> {
        let mut left = self.term()?;
        while let Some(Tok::Op(op @ ('+' | '-'))) = self.peek().cloned() {
            self.pos += 1;
            left = Node::Bin(op, Box::new(left), Box::new(self.term()?));
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<Node, String> {
        let mut left = self.unary()?;
        while let Some(Tok::Op(op @ ('*' | '/'))) = self.peek().cloned() {
            self.pos += 1;
            left = Node::Bin(op, Box::new(left), Box::new(self.unary()?));
        }
        Ok(left)
    }

    /// El signo liga menos que la potencia: -2^2 = -(2^2)
    fn unary(&mut self) -> Result<Node, String> {
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.pos += 1;
                Ok(Node::Neg(Box::new(self.unary()?)))
            }
            Some(Tok::Op('+')) => {
                self.pos += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<Node, String> {
        let base = self.atom()?;
        if let Some(Tok::Op('^')) = self.peek() {
            self.pos += 1;
            // Asociativa a la derecha (2^3^2 = 2^9); el exponente puede llevar signo
            return Ok(Node::Bin('^', Box::new(base), Box::new(self.unary()?)));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<Node, String> {
        match self.next() {
            Some(Tok::Num(v)) => Ok(Node::Num(v)),
            Some(Tok::LParen) => {
                let e = self.expr()?;
                match self.next() {
                    Some(Tok::RParen) => Ok(e),
                    _ => Err("falta cerrar un paréntesis".into()),
                }
            }
            Some(Tok::Ident(name)) => {
                if let Some(Tok::LParen) = self.peek() {
                    self.pos += 1;
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Tok::RParen)) {
                        loop {
                            args.push(self.expr()?);
                            match self.next() {
                                Some(Tok::Comma) => continue,
                                Some(Tok::RParen) => break,
                                _ => return Err(format!("falta cerrar «{name}(»")),
                            }
                        }
                    } else {
                        self.pos += 1;
                    }
                    Ok(Node::Call(name, args))
                } else {
                    Ok(Node::Var(name))
                }
            }
            Some(t) => Err(format!("no se esperaba {}", describe(&t))),
            None => Err("la expresión está incompleta".into()),
        }
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Num(v) => format!("el número {v}"),
        Tok::Ident(s) => format!("«{s}»"),
        Tok::Op(c) => format!("«{c}»"),
        Tok::LParen => "«(»".into(),
        Tok::RParen => "«)»".into(),
        Tok::Comma => "una coma".into(),
    }
}

/// Expresión ya analizada.
#[derive(Debug, Clone)]
pub struct Expr(Node);

impl Expr {
    pub fn parse(src: &str) -> Result<Expr, String> {
        let toks = tokenize(src)?;
        if toks.is_empty() {
            return Err("la expresión está vacía".into());
        }
        let mut p = Parser { toks, pos: 0 };
        let node = p.expr()?;
        if p.pos < p.toks.len() {
            return Err(format!("sobra {}", describe(&p.toks[p.pos])));
        }
        Ok(Expr(node))
    }

    /// Nombres de parámetros que usa (sin constantes ni funciones).
    pub fn variables(&self) -> Vec<String> {
        fn walk(n: &Node, out: &mut Vec<String>) {
            match n {
                Node::Var(v) if !is_constant(v) => {
                    if !out.contains(v) {
                        out.push(v.clone())
                    }
                }
                Node::Neg(a) => walk(a, out),
                Node::Bin(_, a, b) => {
                    walk(a, out);
                    walk(b, out)
                }
                Node::Call(_, args) => args.iter().for_each(|a| walk(a, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        walk(&self.0, &mut out);
        out
    }

    pub fn eval(&self, vars: &HashMap<String, f64>) -> Result<f64, String> {
        let v = eval(&self.0, vars)?;
        if v.is_finite() { Ok(v) } else { Err("el resultado no es un número finito".into()) }
    }
}

fn is_constant(name: &str) -> bool {
    matches!(name, "pi" | "PI" | "π" | "e")
}

fn eval(n: &Node, vars: &HashMap<String, f64>) -> Result<f64, String> {
    Ok(match n {
        Node::Num(v) => *v,
        Node::Var(name) => match name.as_str() {
            "pi" | "PI" | "π" => std::f64::consts::PI,
            "e" => std::f64::consts::E,
            _ => *vars.get(name).ok_or_else(|| format!("no hay un parámetro «{name}»"))?,
        },
        Node::Neg(a) => -eval(a, vars)?,
        Node::Bin(op, a, b) => {
            let (x, y) = (eval(a, vars)?, eval(b, vars)?);
            match op {
                '+' => x + y,
                '-' => x - y,
                '*' => x * y,
                '/' => {
                    if y == 0.0 {
                        return Err("división por cero".into());
                    }
                    x / y
                }
                _ => x.powf(y),
            }
        }
        Node::Call(f, args) => {
            let a: Vec<f64> = args.iter().map(|x| eval(x, vars)).collect::<Result<_, _>>()?;
            let one = |name: &str| -> Result<f64, String> {
                if a.len() == 1 { Ok(a[0]) } else { Err(format!("«{name}» lleva un argumento")) }
            };
            match f.as_str() {
                "sin" => one(f)?.to_radians().sin(),
                "cos" => one(f)?.to_radians().cos(),
                "tan" => one(f)?.to_radians().tan(),
                "asin" => one(f)?.asin().to_degrees(),
                "acos" => one(f)?.acos().to_degrees(),
                "atan" => one(f)?.atan().to_degrees(),
                "sqrt" => {
                    let v = one(f)?;
                    if v < 0.0 {
                        return Err("raíz de un número negativo".into());
                    }
                    v.sqrt()
                }
                "abs" => one(f)?.abs(),
                "round" => one(f)?.round(),
                "floor" => one(f)?.floor(),
                "ceil" => one(f)?.ceil(),
                "min" | "max" if !a.is_empty() => {
                    let it = a.iter().copied();
                    if f == "min" { it.fold(f64::INFINITY, f64::min) } else { it.fold(f64::NEG_INFINITY, f64::max) }
                }
                _ => return Err(format!("no conozco la función «{f}»")),
            }
        }
    })
}

/// ¿Es un nombre de parámetro válido? (letra o _, después letras, dígitos o _)
pub fn valid_name(name: &str) -> bool {
    let mut c = name.chars();
    c.next().is_some_and(|f| f.is_alphabetic() || f == '_')
        && c.all(|x| x.is_alphanumeric() || x == '_')
        && !is_constant(name)
        && !matches!(name, "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sqrt" | "abs" | "round" | "floor" | "ceil" | "min" | "max")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: &str) -> f64 {
        let vars: HashMap<String, f64> = [("ancho".to_string(), 40.0), ("alto".to_string(), 10.0)].into();
        Expr::parse(s).unwrap().eval(&vars).unwrap()
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(ev("1 + 2 * 3"), 7.0);
        assert_eq!(ev("(1 + 2) * 3"), 9.0);
        assert_eq!(ev("-2^2"), -4.0);
        assert_eq!(ev("2^3^2"), 512.0);
        assert_eq!(ev("ancho / 2 - alto"), 10.0);
        assert_eq!(ev("2,5 * 2"), 5.0);
        assert_eq!(ev("10 mm + 5mm"), 15.0);
        assert_eq!(ev("1e3"), 1000.0);
    }

    #[test]
    fn functions_and_constants() {
        assert!((ev("sin(30)") - 0.5).abs() < 1e-12);
        assert!((ev("2 * pi") - std::f64::consts::TAU).abs() < 1e-12);
        assert_eq!(ev("max(ancho, alto, 50)"), 50.0);
        assert_eq!(ev("min(ancho; alto)"), 10.0);
        assert_eq!(ev("max(1,2)"), 2.0);
        // Dentro de una función la coma separa: decimales con punto
        assert_eq!(ev("max(1,5, 2)"), 5.0);
        assert_eq!(ev("max(1.5, 1)"), 1.5);
        assert_eq!(ev("sqrt(16) + abs(-1)"), 5.0);
    }

    #[test]
    fn units() {
        assert_eq!(ev("1 in"), 25.4);
        assert_eq!(ev("2cm + 3 mm"), 23.0);
        assert_eq!(ev("0.5 m"), 500.0);
        assert_eq!(ev("1 ft"), 304.8);
        assert_eq!(ev("2\""), 50.8);
        assert_eq!(ev("45 deg + 15°"), 60.0);
        assert!((ev("pi/2") - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!((ev("1 rad") - 57.29577951308232).abs() < 1e-9);
        // La unidad va con el número: ancho / (2 in)
        assert!((ev("ancho / 2 in") - 40.0 / 50.8).abs() < 1e-12);
        // Una palabra que empieza como unidad no es unidad
        assert!(Expr::parse("2 mmx").is_err());
    }

    #[test]
    fn errors_are_readable() {
        let vars = HashMap::new();
        assert!(Expr::parse("2 +").unwrap_err().contains("incompleta"));
        assert!(Expr::parse("(2").unwrap_err().contains("paréntesis"));
        assert!(Expr::parse("2 $ 3").unwrap_err().contains("$"));
        assert!(Expr::parse("largo * 2").unwrap().eval(&vars).unwrap_err().contains("largo"));
        assert!(Expr::parse("1/0").unwrap().eval(&vars).unwrap_err().contains("cero"));
        assert_eq!(Expr::parse("ancho * alto + pi").unwrap().variables(), vec!["ancho", "alto"]);
        assert!(valid_name("ancho_2") && !valid_name("2x") && !valid_name("pi") && !valid_name("max"));
    }
}
