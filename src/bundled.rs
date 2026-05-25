//! Bundled lava function impls. Register with `register_bundled_functions`.

use crate::{ArgKind, Function, FunctionError, FunctionRegistry, Signature, Value};

/// Register every bundled function in the supplied registry.
pub fn register_bundled_functions(r: &mut FunctionRegistry) {
    r.register(Box::new(Lower));
    r.register(Box::new(Upper));
    r.register(Box::new(Kebab));
    r.register(Box::new(Pascal));
    r.register(Box::new(Snake));
    r.register(Box::new(CidrAdd));
    r.register(Box::new(CidrSubnet));
    r.register(Box::new(HostnameJoin));
    r.register(Box::new(HashBlake3));
    r.register(Box::new(Join));
    r.register(Box::new(DefaultIfEmpty));
}

// ── Case-shape conversions ─────────────────────────────────────────

#[derive(Debug)]
pub struct Lower;
impl Function for Lower {
    fn signature(&self) -> Signature {
        Signature::new("lower", vec![ArgKind::String], ArgKind::String)
    }
    fn call(&self, args: &[Value]) -> Result<Value, FunctionError> {
        Ok(Value::s(args[0].as_str().unwrap_or("").to_ascii_lowercase()))
    }
}

#[derive(Debug)]
pub struct Upper;
impl Function for Upper {
    fn signature(&self) -> Signature {
        Signature::new("upper", vec![ArgKind::String], ArgKind::String)
    }
    fn call(&self, args: &[Value]) -> Result<Value, FunctionError> {
        Ok(Value::s(args[0].as_str().unwrap_or("").to_ascii_uppercase()))
    }
}

#[derive(Debug)]
pub struct Kebab;
impl Function for Kebab {
    fn signature(&self) -> Signature {
        Signature::new("kebab", vec![ArgKind::String], ArgKind::String)
    }
    fn call(&self, args: &[Value]) -> Result<Value, FunctionError> {
        Ok(Value::s(to_kebab(args[0].as_str().unwrap_or(""))))
    }
}

#[derive(Debug)]
pub struct Snake;
impl Function for Snake {
    fn signature(&self) -> Signature {
        Signature::new("snake", vec![ArgKind::String], ArgKind::String)
    }
    fn call(&self, args: &[Value]) -> Result<Value, FunctionError> {
        Ok(Value::s(to_kebab(args[0].as_str().unwrap_or("")).replace('-', "_")))
    }
}

#[derive(Debug)]
pub struct Pascal;
impl Function for Pascal {
    fn signature(&self) -> Signature {
        Signature::new("pascal", vec![ArgKind::String], ArgKind::String)
    }
    fn call(&self, args: &[Value]) -> Result<Value, FunctionError> {
        Ok(Value::s(to_pascal(args[0].as_str().unwrap_or(""))))
    }
}

fn to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev_lower = false;
    for c in s.chars() {
        if c == '_' || c == ' ' {
            out.push('-');
            prev_lower = false;
        } else if c.is_ascii_uppercase() {
            if prev_lower {
                out.push('-');
            }
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            prev_lower = false;
        } else {
            out.push(c);
            prev_lower = c.is_alphanumeric();
        }
    }
    out
}

fn to_pascal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for c in s.chars() {
        if c == '-' || c == '_' || c == ' ' {
            capitalize_next = true;
        } else if capitalize_next {
            out.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

// ── CIDR math ──────────────────────────────────────────────────────

#[derive(Debug)]
pub struct CidrAdd;
impl Function for CidrAdd {
    fn signature(&self) -> Signature {
        Signature::new(
            "cidr-add",
            vec![ArgKind::CidrBlock, ArgKind::Integer],
            ArgKind::CidrBlock,
        )
    }
    fn call(&self, args: &[Value]) -> Result<Value, FunctionError> {
        let cidr = args[0].as_str().unwrap_or("");
        let offset = args[1].as_int().unwrap_or(0);
        cidr_shift_third_octet(cidr, offset)
            .map(Value::String)
            .map_err(|msg| FunctionError::Domain {
                name: "cidr-add".into(),
                message: msg,
            })
    }
}

#[derive(Debug)]
pub struct CidrSubnet;
impl Function for CidrSubnet {
    fn signature(&self) -> Signature {
        Signature::new(
            "cidr-subnet",
            vec![ArgKind::CidrBlock, ArgKind::Integer, ArgKind::Integer],
            ArgKind::CidrBlock,
        )
    }
    fn call(&self, args: &[Value]) -> Result<Value, FunctionError> {
        // Terraform-equivalent cidrsubnet: split base CIDR by adding
        // new-bits to the prefix length, then offset within that slot.
        let base = args[0].as_str().unwrap_or("");
        let new_bits = args[1].as_int().unwrap_or(0);
        let index = args[2].as_int().unwrap_or(0);
        cidr_subnet(base, new_bits, index).map(Value::String).map_err(|m| {
            FunctionError::Domain {
                name: "cidr-subnet".into(),
                message: m,
            }
        })
    }
}

fn cidr_shift_third_octet(cidr: &str, offset: i64) -> Result<String, String> {
    let (network, prefix) = cidr
        .split_once('/')
        .ok_or_else(|| format!("not a CIDR: `{cidr}`"))?;
    let octets: Vec<&str> = network.split('.').collect();
    if octets.len() != 4 {
        return Err(format!("not an IPv4 CIDR: `{cidr}`"));
    }
    let third: i64 = octets[2]
        .parse()
        .map_err(|_| format!("non-numeric octet in `{cidr}`"))?;
    let new_third = third + offset;
    if !(0..=255).contains(&new_third) {
        return Err(format!("octet overflow at offset {offset} on `{cidr}`"));
    }
    let mut out = String::new();
    out.push_str(octets[0]);
    out.push('.');
    out.push_str(octets[1]);
    out.push('.');
    out.push_str(&new_third.to_string());
    out.push('.');
    out.push_str(octets[3]);
    out.push('/');
    out.push_str(prefix);
    Ok(out)
}

fn cidr_subnet(base: &str, new_bits: i64, index: i64) -> Result<String, String> {
    let (network, prefix) = base
        .split_once('/')
        .ok_or_else(|| format!("not a CIDR: `{base}`"))?;
    let prefix: i64 = prefix
        .parse()
        .map_err(|_| format!("non-numeric prefix in `{base}`"))?;
    let new_prefix = prefix + new_bits;
    if !(0..=32).contains(&new_prefix) {
        return Err(format!("prefix overflow: {prefix} + {new_bits} = {new_prefix}"));
    }
    let max_subnets = 1_i64 << new_bits;
    if !(0..max_subnets).contains(&index) {
        return Err(format!(
            "index {index} out of range for new-bits {new_bits} (max {})",
            max_subnets - 1
        ));
    }
    // For the common /16 → /24 case (new_bits = 8) we shift the third
    // octet by index. General formula: shift bits from position
    // (32 - new_prefix) by index.
    let octets: Vec<&str> = network.split('.').collect();
    if octets.len() != 4 {
        return Err(format!("not an IPv4 CIDR: `{base}`"));
    }
    let bytes: [u8; 4] = [
        octets[0].parse().map_err(|_| "bad octet 0".to_string())?,
        octets[1].parse().map_err(|_| "bad octet 1".to_string())?,
        octets[2].parse().map_err(|_| "bad octet 2".to_string())?,
        octets[3].parse().map_err(|_| "bad octet 3".to_string())?,
    ];
    let base_u32 = u32::from_be_bytes(bytes);
    let shift = (32 - new_prefix) as u32;
    let new_u32 = base_u32 | ((index as u32) << shift);
    let nb = new_u32.to_be_bytes();
    let mut out = String::new();
    use std::fmt::Write;
    let _ = write!(
        out,
        "{}.{}.{}.{}/{new_prefix}",
        nb[0], nb[1], nb[2], nb[3]
    );
    Ok(out)
}

// ── Hostname ───────────────────────────────────────────────────────

#[derive(Debug)]
pub struct HostnameJoin;
impl Function for HostnameJoin {
    fn signature(&self) -> Signature {
        Signature::new(
            "hostname-join",
            vec![ArgKind::String, ArgKind::Hostname],
            ArgKind::Hostname,
        )
    }
    fn call(&self, args: &[Value]) -> Result<Value, FunctionError> {
        let sub = args[0].as_str().unwrap_or("");
        let base = args[1].as_str().unwrap_or("");
        let mut out = String::with_capacity(sub.len() + base.len() + 1);
        out.push_str(sub);
        if !sub.is_empty() && !base.is_empty() {
            out.push('.');
        }
        out.push_str(base);
        Ok(Value::String(out))
    }
}

// ── Hashing ────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct HashBlake3;
impl Function for HashBlake3 {
    fn signature(&self) -> Signature {
        Signature::new("hash-blake3", vec![ArgKind::String], ArgKind::String)
    }
    fn call(&self, args: &[Value]) -> Result<Value, FunctionError> {
        let hex = blake3::hash(args[0].as_str().unwrap_or("").as_bytes()).to_hex();
        Ok(Value::String(hex.to_string()))
    }
}

// ── Composition helpers ────────────────────────────────────────────

#[derive(Debug)]
pub struct Join;
impl Function for Join {
    fn signature(&self) -> Signature {
        Signature::new("join", vec![ArgKind::String, ArgKind::List], ArgKind::String)
    }
    fn call(&self, args: &[Value]) -> Result<Value, FunctionError> {
        let sep = args[0].as_str().unwrap_or("");
        let items = args[1].as_list().unwrap_or(&[]);
        Ok(Value::String(items.join(sep)))
    }
}

#[derive(Debug)]
pub struct DefaultIfEmpty;
impl Function for DefaultIfEmpty {
    fn signature(&self) -> Signature {
        Signature::new(
            "default",
            vec![ArgKind::OptionalString, ArgKind::String],
            ArgKind::String,
        )
    }
    fn call(&self, args: &[Value]) -> Result<Value, FunctionError> {
        let a = match &args[0] {
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };
        Ok(Value::String(a.unwrap_or_else(|| {
            args[1].as_str().unwrap_or("").to_string()
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FunctionRegistry;

    fn registry() -> FunctionRegistry {
        let mut r = FunctionRegistry::new();
        register_bundled_functions(&mut r);
        r
    }

    #[test]
    fn case_conversions_are_correct() {
        let r = registry();
        assert_eq!(
            r.call("kebab", &[Value::s("FooBarBaz")]).unwrap(),
            Value::s("foo-bar-baz")
        );
        assert_eq!(
            r.call("snake", &[Value::s("FooBarBaz")]).unwrap(),
            Value::s("foo_bar_baz")
        );
        assert_eq!(
            r.call("pascal", &[Value::s("aws-vpc-network")]).unwrap(),
            Value::s("AwsVpcNetwork")
        );
        assert_eq!(
            r.call("upper", &[Value::s("hi")]).unwrap(),
            Value::s("HI")
        );
        assert_eq!(
            r.call("lower", &[Value::s("HI")]).unwrap(),
            Value::s("hi")
        );
    }

    #[test]
    fn cidr_add_shifts_third_octet() {
        let r = registry();
        assert_eq!(
            r.call("cidr-add", &[Value::s("10.0.0.0/16"), Value::Integer(5)])
                .unwrap(),
            Value::s("10.0.5.0/16")
        );
    }

    #[test]
    fn cidr_add_surfaces_typed_domain_error_on_overflow() {
        let r = registry();
        let err = r
            .call("cidr-add", &[Value::s("10.0.250.0/16"), Value::Integer(99)])
            .unwrap_err();
        match err {
            FunctionError::Domain { name, .. } => assert_eq!(name, "cidr-add"),
            other => panic!("expected Domain, got {other:?}"),
        }
    }

    #[test]
    fn cidr_subnet_splits_by_new_bits() {
        let r = registry();
        // /16 + 8 new-bits = /24; index 5 → 10.0.5.0/24
        assert_eq!(
            r.call(
                "cidr-subnet",
                &[Value::s("10.0.0.0/16"), Value::Integer(8), Value::Integer(5)]
            )
            .unwrap(),
            Value::s("10.0.5.0/24")
        );
    }

    #[test]
    fn hostname_join_dots_subdomain_and_base() {
        let r = registry();
        assert_eq!(
            r.call(
                "hostname-join",
                &[Value::s("api"), Value::s("example.com")]
            )
            .unwrap(),
            Value::s("api.example.com")
        );
    }

    #[test]
    fn hash_blake3_is_stable_for_same_input() {
        let r = registry();
        let a = r.call("hash-blake3", &[Value::s("seed")]).unwrap();
        let b = r.call("hash-blake3", &[Value::s("seed")]).unwrap();
        assert_eq!(a, b);
        // Hex output = 64 chars.
        assert_eq!(a.as_str().unwrap().len(), 64);
    }

    #[test]
    fn join_collapses_list_with_separator() {
        let r = registry();
        let items = Value::List(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(
            r.call("join", &[Value::s(", "), items]).unwrap(),
            Value::s("a, b, c")
        );
    }

    #[test]
    fn default_returns_a_when_nonempty_else_b() {
        let r = registry();
        assert_eq!(
            r.call("default", &[Value::s("here"), Value::s("fallback")])
                .unwrap(),
            Value::s("here")
        );
        assert_eq!(
            r.call("default", &[Value::s(""), Value::s("fallback")])
                .unwrap(),
            Value::s("fallback")
        );
        assert_eq!(
            r.call("default", &[Value::Null, Value::s("fallback")])
                .unwrap(),
            Value::s("fallback")
        );
    }

    #[test]
    fn bundled_function_names_are_all_registered() {
        let r = registry();
        for name in [
            "lower", "upper", "kebab", "pascal", "snake", "cidr-add",
            "cidr-subnet", "hostname-join", "hash-blake3", "join", "default",
        ] {
            assert!(r.get(name).is_some(), "missing bundled function `{name}`");
        }
    }
}
