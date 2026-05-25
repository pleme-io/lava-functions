# lava-functions

Typed pure transformations for the lava + tatara-lisp ecosystem.

## Bundled functions

| Name             | Signature                                  |
|---|---|
| `lower`          | (s: str) → str                             |
| `upper`          | (s: str) → str                             |
| `kebab`          | (s: str) → str                             |
| `pascal`         | (s: str) → str                             |
| `snake`          | (s: str) → str                             |
| `cidr-add`       | (base: cidr, offset: int) → cidr           |
| `cidr-subnet`    | (base: cidr, new-bits: int, index: int) → cidr |
| `hostname-join`  | (sub: str, base: hostname) → hostname      |
| `hash-blake3`    | (s: str) → str                             |
| `join`           | (sep: str, items: list&lt;str&gt;) → str   |
| `default`        | (a: opt&lt;str&gt;, b: str) → str          |

## Typed surface

```rust
use lava_functions::{FunctionRegistry, Value};
use lava_functions::bundled::register_bundled_functions;

let mut r = FunctionRegistry::new();
register_bundled_functions(&mut r);

let subnet = r.call(
    "cidr-subnet",
    &[
        Value::s("10.0.0.0/16"),
        Value::Integer(8),
        Value::Integer(3),
    ],
)?;
assert_eq!(subnet.as_str(), Some("10.0.3.0/24"));
```

## Custom functions

```rust
use lava_functions::{ArgKind, Function, FunctionError, Signature, Value};

#[derive(Debug)]
struct Reverse;

impl Function for Reverse {
    fn signature(&self) -> Signature {
        Signature::new("reverse", vec![ArgKind::String], ArgKind::String)
    }
    fn call(&self, args: &[Value]) -> Result<Value, FunctionError> {
        Ok(Value::s(args[0].as_str().unwrap_or("").chars().rev().collect::<String>()))
    }
}

r.register(Box::new(Reverse));
```

The `FunctionRegistry::call` gate validates arity + argument kind
before invoking the impl — typed `FunctionError::Arity` /
`TypeMismatch` / `Unknown` surface up to the caller without ever
entering the body.
