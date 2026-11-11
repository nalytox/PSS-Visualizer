//! Diferencias entre valores JSON. En la traza compacta (versión 2) cada paso y cada instantánea se
//! guarda como el cambio respecto del anterior. Un parche es:
//!
//! - un valor primitivo o `null`: el valor nuevo;
//! - `{"$s": v}`: reemplazo por `v` (así llegan los objetos y arreglos nuevos);
//! - en un objeto, `{"clave": parche, ...}`, con `{"$d": 1}` para borrar una clave;
//! - en un arreglo, `{"$a": largo, "3": parche, ...}`; `$a` falta si el largo no cambió.
//!
//! Ninguna clave de la traza empieza con `$`, así que un parche no se confunde con un objeto.

use crate::{Frame, HeapBlock, MemorySnapshot, Value as TraceValue, Var};
use serde::Serialize;
use serde_json::{Map, Value};

/// Parche que convierte `a` en `b`, o `None` si son iguales.
pub fn diff(a: &Value, b: &Value) -> Option<Value> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut p = Map::new();
            for (k, vb) in y {
                match x.get(k) {
                    Some(va) => {
                        if let Some(d) = diff(va, vb) {
                            p.insert(k.clone(), d);
                        }
                    }
                    None => {
                        p.insert(k.clone(), set(vb));
                    }
                }
            }
            for k in x.keys().filter(|k| !y.contains_key(*k)) {
                p.insert(k.clone(), deleted());
            }
            (!p.is_empty()).then_some(Value::Object(p))
        }
        // Un arreglo que estaba vacío (los eventos de un paso) llega entero.
        (Value::Array(x), Value::Array(y)) if !x.is_empty() => {
            let mut p = Map::new();
            if x.len() != y.len() {
                p.insert("$a".into(), y.len().into());
            }
            for (i, vb) in y.iter().enumerate() {
                match x.get(i) {
                    Some(va) => {
                        if let Some(d) = diff(va, vb) {
                            p.insert(i.to_string(), d);
                        }
                    }
                    None => {
                        p.insert(i.to_string(), set(vb));
                    }
                }
            }
            (!p.is_empty()).then_some(Value::Object(p))
        }
        _ if a == b => None,
        _ => Some(set(b)),
    }
}

/// Aplica un parche producido por [`diff`].
pub fn apply(a: &Value, p: &Value) -> Value {
    let Value::Object(pm) = p else {
        return p.clone();
    };
    if let Some(v) = pm.get("$s") {
        return v.clone();
    }
    match a {
        Value::Array(items) => {
            let mut out = items.clone();
            if let Some(n) = pm.get("$a").and_then(Value::as_u64) {
                out.resize(n as usize, Value::Null);
            }
            for (k, sub) in pm.iter().filter(|(k, _)| *k != "$a") {
                if let Ok(i) = k.parse::<usize>()
                    && i < out.len()
                {
                    out[i] = apply(&out[i], sub);
                }
            }
            Value::Array(out)
        }
        Value::Object(fields) => {
            let mut out = fields.clone();
            for (k, sub) in pm {
                if is_deleted(sub) {
                    out.remove(k);
                } else {
                    let next = apply(out.get(k).unwrap_or(&Value::Null), sub);
                    out.insert(k.clone(), next);
                }
            }
            Value::Object(out)
        }
        _ => p.clone(),
    }
}

fn set(v: &Value) -> Value {
    match v {
        Value::Object(_) | Value::Array(_) => {
            let mut m = Map::new();
            m.insert("$s".into(), v.clone());
            Value::Object(m)
        }
        _ => v.clone(),
    }
}

fn deleted() -> Value {
    let mut m = Map::new();
    m.insert("$d".into(), 1.into());
    Value::Object(m)
}

fn is_deleted(p: &Value) -> bool {
    p.as_object().is_some_and(|m| m.len() == 1 && m.contains_key("$d"))
}

/// Parche entre dos instantáneas, comparando primero sus tipos: solo se convierte a JSON lo que
/// cambió. Equivale a `diff` sobre su JSON (lo comprueban las pruebas), pero un paso que cambia un
/// elemento de un arreglo grande no recorre el arreglo entero dos veces.
pub fn snapshot_patch(a: &MemorySnapshot, b: &MemorySnapshot) -> Option<Value> {
    let mut p = Map::new();
    if let Some(d) = vec_patch(&a.globals, &b.globals, var_patch) {
        p.insert("globals".into(), d);
    }
    let mut stacks = Map::new();
    for (tid, fb) in &b.stacks {
        match a.stacks.get(tid) {
            Some(fa) => {
                if let Some(d) = vec_patch(fa, fb, frame_patch) {
                    stacks.insert(tid.to_string(), d);
                }
            }
            None => {
                stacks.insert(tid.to_string(), set(&json(fb)));
            }
        }
    }
    for tid in a.stacks.keys().filter(|t| !b.stacks.contains_key(*t)) {
        stacks.insert(tid.to_string(), deleted());
    }
    if !stacks.is_empty() {
        p.insert("stacks".into(), Value::Object(stacks));
    }
    if let Some(d) = vec_patch(&a.heap, &b.heap, block_patch) {
        p.insert("heap".into(), d);
    }
    (!p.is_empty()).then_some(Value::Object(p))
}

fn json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("un valor de la traza se serializa")
}

fn whole<T: Serialize + PartialEq>(a: &T, b: &T) -> Option<Value> {
    if a == b { None } else { diff(&json(a), &json(b)) }
}

fn vec_patch<T: Serialize + PartialEq>(a: &[T], b: &[T], item: fn(&T, &T) -> Option<Value>) -> Option<Value> {
    if a.len() != b.len() || a.is_empty() {
        return whole(&a.to_vec_ref(), &b.to_vec_ref());
    }
    let mut p = Map::new();
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if x != y
            && let Some(d) = item(x, y)
        {
            p.insert(i.to_string(), d);
        }
    }
    (!p.is_empty()).then_some(Value::Object(p))
}

/// Vista de un slice que se serializa como arreglo (para comparar y convertir sin copiar).
trait AsVecRef<T> {
    fn to_vec_ref(&self) -> SliceRef<'_, T>;
}

impl<T> AsVecRef<T> for [T] {
    fn to_vec_ref(&self) -> SliceRef<'_, T> {
        SliceRef(self)
    }
}

#[derive(PartialEq)]
struct SliceRef<'a, T>(&'a [T]);

impl<T: Serialize> Serialize for SliceRef<'_, T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

fn single(key: &str, v: Value) -> Value {
    let mut m = Map::new();
    m.insert(key.into(), v);
    Value::Object(m)
}

fn var_patch(a: &Var, b: &Var) -> Option<Value> {
    if a.name == b.name && a.ty == b.ty && a.addr == b.addr && a.size == b.size && a.uninit == b.uninit {
        value_patch(&a.value, &b.value).map(|d| single("value", d))
    } else {
        whole(a, b)
    }
}

fn frame_patch(a: &Frame, b: &Frame) -> Option<Value> {
    if a.func != b.func || a.signal != b.signal {
        return whole(a, b);
    }
    let mut p = Map::new();
    if a.line != b.line {
        p.insert("line".into(), b.line.into());
    }
    if let Some(d) = vec_patch(&a.params, &b.params, var_patch) {
        p.insert("params".into(), d);
    }
    if let Some(d) = vec_patch(&a.locals, &b.locals, var_patch) {
        p.insert("locals".into(), d);
    }
    (!p.is_empty()).then_some(Value::Object(p))
}

fn block_patch(a: &HeapBlock, b: &HeapBlock) -> Option<Value> {
    if a.addr == b.addr
        && a.size == b.size
        && a.ty == b.ty
        && a.alloc_at == b.alloc_at
        && a.alloc_line == b.alloc_line
        && a.freed_at == b.freed_at
    {
        value_patch(&a.value, &b.value).map(|d| single("value", d))
    } else {
        whole(a, b)
    }
}

fn value_patch(a: &TraceValue, b: &TraceValue) -> Option<Value> {
    match (a, b) {
        (
            TraceValue::Array {
                length: la,
                items: ia,
                text: ta,
            },
            TraceValue::Array {
                length: lb,
                items: ib,
                text: tb,
            },
        ) if la == lb && ta.is_some() == tb.is_some() => {
            let mut p = Map::new();
            if let Some(d) = vec_patch(ia, ib, value_patch) {
                p.insert("items".into(), d);
            }
            if ta != tb
                && let Some(t) = tb
            {
                p.insert("text".into(), Value::String(t.clone()));
            }
            (!p.is_empty()).then_some(Value::Object(p))
        }
        (TraceValue::Struct { fields: fa }, TraceValue::Struct { fields: fb })
        | (TraceValue::Union { fields: fa }, TraceValue::Union { fields: fb }) => {
            vec_patch(fa, fb, var_patch).map(|d| single("fields", d))
        }
        _ => whole(a, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn roundtrip(a: Value, b: Value) -> Option<Value> {
        let p = diff(&a, &b);
        let back = p.as_ref().map_or(a.clone(), |p| apply(&a, p));
        assert_eq!(back, b, "parche {p:?}");
        p
    }

    #[test]
    fn equal_values_need_no_patch() {
        assert_eq!(
            roundtrip(json!({"a": [1, {"b": null}]}), json!({"a": [1, {"b": null}]})),
            None
        );
    }

    #[test]
    fn only_what_changed_is_kept() {
        let p = roundtrip(
            json!({"t": 4, "processes": [{"pid": 1000, "threads": [{"line": 7}], "mem": "m3"}]}),
            json!({"t": 5, "processes": [{"pid": 1000, "threads": [{"line": 8}], "mem": "m4"}]}),
        );
        assert_eq!(
            p,
            Some(json!({"t": 5, "processes": {"0": {"threads": {"0": {"line": 8}}, "mem": "m4"}}}))
        );
    }

    #[test]
    fn keys_and_items_come_and_go() {
        roundtrip(
            json!({"a": 1, "b": {"c": 2}}),
            json!({"b": {"c": 2, "d": [1, 2]}, "e": null}),
        );
        roundtrip(json!([1, 2, 3]), json!([1]));
        roundtrip(json!([1]), json!([1, {"x": 1}, [2]]));
        roundtrip(json!([]), json!([{"type": "fork"}]));
        roundtrip(json!([{"type": "fork"}]), json!([]));
    }

    #[test]
    fn a_value_can_change_its_kind() {
        roundtrip(json!({"mem": null}), json!({"mem": "m1"}));
        roundtrip(json!({"v": {"kind": "scalar"}}), json!({"v": [1]}));
        roundtrip(json!({"v": [1]}), json!({"v": 3}));
        roundtrip(json!({"v": 3}), json!({"v": {"k": 1}}));
    }
}
