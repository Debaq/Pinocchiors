//! Esquemas JSON de los argumentos (el subconjunto que usan las herramientas)
//! y su verificación, con errores que dicen qué falta y qué se esperaba.

use serde_json::{json, Map, Value};

/// Propiedad de un objeto: nombre, esquema y si es obligatoria
pub type Prop = (&'static str, Value, bool);

/// Objeto cerrado: solo esas propiedades
pub fn object(props: &[Prop]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for (name, schema, req) in props {
        properties.insert((*name).into(), schema.clone());
        if *req {
            required.push(json!(name));
        }
    }
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

pub fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

pub fn boolean(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

pub fn number(description: &str) -> Value {
    json!({ "type": "number", "description": description })
}

/// Número con límites (inclusivos)
pub fn number_in(description: &str, min: f64, max: f64) -> Value {
    json!({ "type": "number", "description": description, "minimum": min, "maximum": max })
}

pub fn integer(description: &str, min: i64) -> Value {
    json!({ "type": "integer", "description": description, "minimum": min })
}

/// Una de esas palabras
pub fn one_of(description: &str, options: &[&str]) -> Value {
    json!({ "type": "string", "description": description, "enum": options })
}

/// Id (número) o nombre (texto) de algo
pub fn id_or_name(description: &str) -> Value {
    json!({ "type": ["integer", "string"], "description": description })
}

/// Objeto abierto (sus campos los verifica quien lo usa), con un ejemplo
pub fn any_object(description: &str) -> Value {
    json!({ "type": "object", "description": description })
}

/// Lista de largo fijo de números: un punto 2D o 3D
pub fn point(description: &str, n: usize) -> Value {
    json!({ "type": "array", "description": description, "items": { "type": "number" }, "minItems": n, "maxItems": n })
}

pub fn array(description: &str, items: Value) -> Value {
    json!({ "type": "array", "description": description, "items": items })
}

/// Verifica `value` contra `schema`; el error nombra el camino (`args.size[2]`)
pub fn check(schema: &Value, value: &Value) -> Result<(), String> {
    check_at(schema, value, "args")
}

fn is_type(ty: &str, value: &Value) -> bool {
    match ty {
        "object" => value.is_object(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "array" => value.is_array(),
        _ => true,
    }
}

fn check_at(schema: &Value, value: &Value, at: &str) -> Result<(), String> {
    // Un tipo o varios (`["integer", "string"]`)
    let types: Vec<&str> = match schema.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    if !types.is_empty() && !types.iter().any(|t| is_type(t, value)) {
        return Err(format!("{at}: expected {}, got {}", types.join(" or "), kind(value)));
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && !options.contains(value)
    {
        let list: Vec<String> = options.iter().map(|o| o.to_string()).collect();
        return Err(format!("{at}: {value} is not an option; use one of {}", list.join(", ")));
    }
    if let Some(x) = value.as_f64() {
        if let Some(min) = schema.get("minimum").and_then(Value::as_f64)
            && x < min
        {
            return Err(format!("{at}: {x} is below the minimum {min}"));
        }
        if let Some(max) = schema.get("maximum").and_then(Value::as_f64)
            && x > max
        {
            return Err(format!("{at}: {x} is above the maximum {max}"));
        }
    }
    if let (Some(obj), Some(props)) = (value.as_object(), schema.get("properties").and_then(Value::as_object)) {
        for req in schema.get("required").and_then(Value::as_array).into_iter().flatten() {
            let name = req.as_str().unwrap_or_default();
            if !obj.contains_key(name) {
                return Err(format!("{at}: missing \"{name}\" ({})", describe(&props[name])));
            }
        }
        for (name, v) in obj {
            match props.get(name) {
                Some(p) => check_at(p, v, &format!("{at}.{name}"))?,
                None if schema.get("additionalProperties") == Some(&json!(false)) => {
                    let known: Vec<&str> = props.keys().map(String::as_str).collect();
                    return Err(format!("{at}: unknown argument \"{name}\"; valid ones: {}", known.join(", ")));
                }
                None => {}
            }
        }
    }
    if let Some(list) = value.as_array() {
        let (min, max) = (schema.get("minItems").and_then(Value::as_u64), schema.get("maxItems").and_then(Value::as_u64));
        let n = list.len() as u64;
        if min.is_some_and(|m| n < m) || max.is_some_and(|m| n > m) {
            let want = if min == max { format!("{}", min.unwrap_or(0)) } else { format!("{}..{}", min.unwrap_or(0), max.map_or("".into(), |m| m.to_string())) };
            return Err(format!("{at}: expected {want} items, got {n}"));
        }
    }
    if let (Some(items), Some(list)) = (schema.get("items"), value.as_array()) {
        for (i, v) in list.iter().enumerate() {
            check_at(items, v, &format!("{at}[{i}]"))?;
        }
    }
    Ok(())
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn describe(schema: &Value) -> String {
    let ty = match schema.get("type") {
        Some(Value::String(t)) => t.clone(),
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" or "),
        _ => "value".into(),
    };
    match schema.get("description").and_then(Value::as_str) {
        Some(d) => format!("{ty}: {d}"),
        None => ty.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_schema() -> Value {
        object(&[
            ("path", string("Where to save"), true),
            ("format", one_of("File format", &["step", "stl"]), false),
            ("size", array("Size in mm", number_in("mm", 0.0, 1000.0)), false),
        ])
    }

    #[test]
    fn accepts_valid_and_names_the_problem() {
        let s = box_schema();
        assert!(check(&s, &json!({ "path": "/tmp/a.step", "format": "step", "size": [1, 2, 3] })).is_ok());
        let e = check(&s, &json!({})).unwrap_err();
        assert!(e.contains("missing \"path\"") && e.contains("Where to save"), "{e}");
        let e = check(&s, &json!({ "path": "a", "colour": 1 })).unwrap_err();
        assert!(e.contains("unknown argument \"colour\"") && e.contains("format"), "{e}");
        let e = check(&s, &json!({ "path": "a", "format": "obj" })).unwrap_err();
        assert!(e.contains("\"step\""), "{e}");
        let e = check(&s, &json!({ "path": "a", "size": [1, -2] })).unwrap_err();
        assert!(e.contains("args.size[1]") && e.contains("minimum"), "{e}");
        let e = check(&s, &json!({ "path": 3 })).unwrap_err();
        assert!(e.contains("expected string"), "{e}");
    }
}
