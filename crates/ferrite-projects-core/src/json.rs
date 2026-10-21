//! Records as the board keeps them on disk: JSON objects read the way the board always read them. A task, an
//! automation or a review is an open record: another machine, or an older version of the board, may have written
//! fields this one does not know, and every one of them is kept when the record is written back. So a record is a
//! JSON object, and these are its plain accessors: a missing field reads as nothing, never as an error.
pub use serde_json::{json, Map, Value};

pub type Obj = Map<String, Value>;
pub static NULL: Value = Value::Null;

/// Whether a value counts as "there": not null, false, 0, NaN or "".
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0 && !f.is_nan()).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
/// A value as words: a string as it is, a whole number without a point, null as nothing.
pub fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => num_text(n.as_f64().unwrap_or(0.0)),
        other => other.to_string(),
    }
}
/// A number as JavaScript writes it: `12`, `1.5`.
pub fn num_text(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e17 { format!("{}", f as i64) } else { format!("{f}") }
}
/// A value as a number: a number, or a string that holds one; anything else is 0.
pub fn number(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse::<f64>().unwrap_or(0.0),
        Value::Bool(b) => if *b { 1.0 } else { 0.0 },
        _ => 0.0,
    }
}

/// Reading a field of a record.
pub trait Js {
    fn at(&self, k: &str) -> &Value;
    /// The field as a string slice; "" when it is not a string.
    fn s(&self, k: &str) -> &str { self.at(k).as_str().unwrap_or("") }
    /// The field as words, whatever it holds.
    fn t(&self, k: &str) -> String { text(self.at(k)) }
    fn f(&self, k: &str) -> f64 { number(self.at(k)) }
    fn i(&self, k: &str) -> i64 { number(self.at(k)) as i64 }
    /// Truthy, as a condition reads it.
    fn b(&self, k: &str) -> bool { truthy(self.at(k)) }
    /// Present and not null.
    fn has(&self, k: &str) -> bool { !self.at(k).is_null() }
    fn arr(&self, k: &str) -> &[Value] { self.at(k).as_array().map(|a| a.as_slice()).unwrap_or(&[]) }
    fn o(&self, k: &str) -> Option<&Obj> { self.at(k).as_object() }
    /// A string field, or None when it is missing or empty.
    fn so(&self, k: &str) -> Option<&str> { self.at(k).as_str().filter(|s| !s.is_empty()) }
}
impl Js for Obj {
    fn at(&self, k: &str) -> &Value { self.get(k).unwrap_or(&NULL) }
}
impl Js for Value {
    fn at(&self, k: &str) -> &Value { self.as_object().and_then(|o| o.get(k)).unwrap_or(&NULL) }
}
impl<T: Js> Js for Option<T> {
    fn at(&self, k: &str) -> &Value { self.as_ref().map(|x| x.at(k)).unwrap_or(&NULL) }
}
impl<T: Js + ?Sized> Js for &T {
    fn at(&self, k: &str) -> &Value { (**self).at(k) }
}

/// Changing a field of a record.
pub trait JsMut {
    fn put(&mut self, k: &str, v: impl Into<Value>);
    fn del(&mut self, k: &str);
    /// The field as an object to change, made an empty one when it is not.
    fn obj_mut(&mut self, k: &str) -> &mut Obj;
}
impl JsMut for Obj {
    fn put(&mut self, k: &str, v: impl Into<Value>) { self.insert(k.to_string(), v.into()); }
    fn del(&mut self, k: &str) { self.shift_remove(k); }
    fn obj_mut(&mut self, k: &str) -> &mut Obj {
        if !self.get(k).map(|v| v.is_object()).unwrap_or(false) { self.insert(k.to_string(), Value::Object(Obj::new())); }
        self.get_mut(k).and_then(|v| v.as_object_mut()).expect("an object was just put there")
    }
}

/// An object from a `json!` value; an empty one when it is not an object.
pub fn obj(v: Value) -> Obj { match v { Value::Object(o) => o, _ => Obj::new() } }
/// `{...a, ...b}`: b's fields over a's.
pub fn merged(a: &Obj, b: &Obj) -> Obj { let mut out = a.clone(); for (k, v) in b { out.insert(k.clone(), v.clone()); } out }
/// Strings of an array field.
pub fn strings(v: &Value) -> Vec<String> { v.as_array().map(|a| a.iter().map(text).collect()).unwrap_or_default() }
/// Whole numbers written as JavaScript writes them (2, not 2.0), all the way down: what goes to disk and to --json.
pub fn tidy(v: Value) -> Value {
    match v {
        Value::Number(n) if n.is_f64() => { let f = n.as_f64().unwrap_or(0.0); if f.fract() == 0.0 && f.abs() < 9.0e15 { json!(f as i64) } else { Value::Number(n) } }
        Value::Array(a) => Value::Array(a.into_iter().map(tidy).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, tidy(v))).collect()),
        v => v,
    }
}
