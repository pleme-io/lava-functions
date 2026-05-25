//! lava-functions — typed pure transformations for the lava +
//! tatara-lisp ecosystem.
//!
//! Each function ships:
//! 1. A typed Rust [`Function`] impl: name + signature + call.
//! 2. A typed [`Signature`] declaring the typed args.
//! 3. A [`FunctionRegistry`] entry consumers look up by name.
//!
//! Functions are pure — same inputs always produce same output. They
//! don't read the filesystem, network, or environment.
//!
//! ## Bundled functions
//!
//! | Name             | Signature                    | Notes                              |
//! |---|---|---|
//! | `lower`          | (s: str) → str               | ASCII lowercase                    |
//! | `upper`          | (s: str) → str               | ASCII uppercase                    |
//! | `kebab`          | (s: str) → str               | foo_bar / fooBar → foo-bar         |
//! | `pascal`         | (s: str) → str               | foo-bar → FooBar                   |
//! | `snake`          | (s: str) → str               | foo-bar / fooBar → foo_bar         |
//! | `cidr-add`       | (base: cidr, offset: int) → cidr | shift the third octet by offset |
//! | `cidr-subnet`    | (base: cidr, new-bits: int, index: int) → cidr | terraform-cidrsubnet shape |
//! | `hostname-join`  | (sub: str, base: hostname) → hostname | "{sub}.{base}"             |
//! | `hash-blake3`    | (s: str) → str               | hex-encoded BLAKE3 of input         |
//! | `format-tags`    | (key=v, key=v, …) → string    | terraform tags-map string shape    |
//! | `join`           | (sep: str, items: list&lt;str&gt;) → str | string join                |
//! | `default`        | (a: opt&lt;str&gt;, b: str) → str | a if non-empty else b             |
//!
//! ## Adding more
//!
//! Implement [`Function`] for any struct + register via
//! [`FunctionRegistry::register`]. The signature gate runs before
//! `call`, so argument-arity / type errors surface as typed
//! [`FunctionError`] variants.

#![allow(clippy::module_name_repetitions)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod bundled;
pub use bundled::register_bundled_functions;

/// Typed argument kind. Consumers (tlisp interpreter, future codegen)
/// match on these to validate before invoking the function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArgKind {
    String,
    Integer,
    CidrBlock,
    Hostname,
    /// Heterogeneous list. Consumer asserts element kind separately.
    List,
    /// Optional string — None / Some(""). Returns default if empty.
    OptionalString,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub name: String,
    pub args: Vec<ArgKind>,
    pub returns: ArgKind,
}

impl Signature {
    #[must_use]
    pub fn new(name: impl Into<String>, args: Vec<ArgKind>, returns: ArgKind) -> Self {
        Self {
            name: name.into(),
            args,
            returns,
        }
    }
}

/// Typed function value passed to / returned from [`Function::call`].
/// Strings are the universal interop type; integers are explicit so
/// the typed signature gate can validate before call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum Value {
    String(String),
    Integer(i64),
    List(Vec<String>),
    Null,
}

impl Value {
    #[must_use]
    pub fn s(v: impl Into<String>) -> Self {
        Self::String(v.into())
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Integer(n) => Some(*n),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_list(&self) -> Option<&[String]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }
}

/// Typed function trait every concrete impl implements. Pure: no
/// side effects, no I/O.
pub trait Function: Send + Sync {
    fn signature(&self) -> Signature;
    /// Invoke the function. Args have already passed signature gate
    /// (arity correct, kinds matched); impls should still defensively
    /// return [`FunctionError::TypeMismatch`] on shape surprises.
    ///
    /// # Errors
    /// Surfaces [`FunctionError`] for argument-shape problems.
    fn call(&self, args: &[Value]) -> Result<Value, FunctionError>;
}

/// Registry of typed functions consumed by the tlisp interpreter +
/// any other surface that needs to look up functions by name.
#[derive(Default)]
pub struct FunctionRegistry {
    by_name: indexmap::IndexMap<String, Box<dyn Function>>,
}

impl std::fmt::Debug for FunctionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FunctionRegistry")
            .field("size", &self.by_name.len())
            .field("names", &self.by_name.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl FunctionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a function by its declared name. Replaces any existing
    /// entry under the same name.
    pub fn register(&mut self, f: Box<dyn Function>) {
        let name = f.signature().name;
        self.by_name.insert(name, f);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Function> {
        self.by_name.get(name).map(std::convert::AsRef::as_ref)
    }

    /// Validate args against the function's declared signature, then
    /// invoke. The two-step gate gives typed errors for arity /
    /// argument-kind mismatches before the function body runs.
    ///
    /// # Errors
    /// Returns [`FunctionError::Unknown`] / [`FunctionError::Arity`] /
    /// [`FunctionError::TypeMismatch`] from the gate, or whatever the
    /// function body surfaces.
    pub fn call(&self, name: &str, args: &[Value]) -> Result<Value, FunctionError> {
        let f = self
            .get(name)
            .ok_or_else(|| FunctionError::Unknown(name.to_string()))?;
        let sig = f.signature();
        if sig.args.len() != args.len() {
            return Err(FunctionError::Arity {
                name: sig.name,
                expected: sig.args.len(),
                got: args.len(),
            });
        }
        for (i, (expected, actual)) in sig.args.iter().zip(args).enumerate() {
            if !kind_accepts(*expected, actual) {
                return Err(FunctionError::TypeMismatch {
                    name: sig.name.clone(),
                    position: i,
                    expected: *expected,
                });
            }
        }
        f.call(args)
    }

    #[must_use]
    pub fn names(&self) -> Vec<&String> {
        self.by_name.keys().collect()
    }
}

#[derive(Debug, Error, Clone, Serialize, Deserialize)]
pub enum FunctionError {
    #[error("unknown function `{0}`")]
    Unknown(String),
    #[error("`{name}` expects {expected} arg(s), got {got}")]
    Arity {
        name: String,
        expected: usize,
        got: usize,
    },
    #[error("`{name}` arg #{position} must be {expected:?}")]
    TypeMismatch {
        name: String,
        position: usize,
        expected: ArgKind,
    },
    #[error("`{name}`: {message}")]
    Domain { name: String, message: String },
}

/// Decide whether the actual [`Value`] satisfies the expected
/// [`ArgKind`]. CidrBlock + Hostname accept String at the kind level
/// (downstream call validates shape via lava-types).
fn kind_accepts(expected: ArgKind, actual: &Value) -> bool {
    match (expected, actual) {
        (ArgKind::String | ArgKind::CidrBlock | ArgKind::Hostname, Value::String(_)) => true,
        (ArgKind::OptionalString, Value::String(_) | Value::Null) => true,
        (ArgKind::Integer, Value::Integer(_)) => true,
        (ArgKind::List, Value::List(_)) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundled::register_bundled_functions;

    #[test]
    fn registry_gate_arity_mismatch_surfaces_typed_error() {
        let mut r = FunctionRegistry::new();
        register_bundled_functions(&mut r);
        let err = r.call("lower", &[]).unwrap_err();
        match err {
            FunctionError::Arity { name, expected, got } => {
                assert_eq!(name, "lower");
                assert_eq!(expected, 1);
                assert_eq!(got, 0);
            }
            other => panic!("expected Arity, got {other:?}"),
        }
    }

    #[test]
    fn registry_gate_unknown_function_surfaces_typed_error() {
        let r = FunctionRegistry::new();
        let err = r.call("nope", &[]).unwrap_err();
        matches!(err, FunctionError::Unknown(_));
    }

    #[test]
    fn registry_gate_kind_mismatch_surfaces_typed_error() {
        let mut r = FunctionRegistry::new();
        register_bundled_functions(&mut r);
        // cidr-add expects (CidrBlock, Integer); pass (Integer, Integer).
        let err = r
            .call("cidr-add", &[Value::Integer(10), Value::Integer(5)])
            .unwrap_err();
        match err {
            FunctionError::TypeMismatch { name, position, .. } => {
                assert_eq!(name, "cidr-add");
                assert_eq!(position, 0);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn value_serde_round_trips() {
        let v = Value::List(vec!["a".into(), "b".into()]);
        let json = serde_json::to_string(&v).unwrap();
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v, parsed);
    }
}
