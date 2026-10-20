//! The `seed` request value (D-c, ADR 0026): a decimal string or a JSON integer up to 2^53 - 1.

use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use utoipa::openapi::schema::{ObjectBuilder, OneOfBuilder, Type};
use utoipa::openapi::{RefOr, Schema};
use utoipa::{PartialSchema, ToSchema};

/// A simulation seed in a request.
///
/// Requests accept `string | integer`: a string of 1 to 20 decimal digits that fits `u64`, or an
/// integer in `[0, 2^53 - 1]` (larger integers are not exact in JavaScript and are refused; send them
/// as strings). Responses always carry the seed as a decimal string (a plain `String` field).
/// Leading zeros are accepted and normalised: `"007"` is seed 7 and comes back as `"7"`.
/// Serialising a `Seed` also gives the decimal string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seed(pub u64);

impl Seed {
    /// The largest integer a request may send as a JSON number: 2^53 - 1.
    pub const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;
}

impl Serialize for Seed {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

struct SeedVisitor;

impl Visitor<'_> for SeedVisitor {
    type Value = Seed;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a decimal string of up to 20 digits, or an integer from 0 to 2^53 - 1")
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Seed, E> {
        if v > Seed::MAX_SAFE_INTEGER {
            return Err(E::custom(format!(
                "seed {v} is above 2^53 - 1: send it as a string"
            )));
        }
        Ok(Seed(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Seed, E> {
        u64::try_from(v)
            .map_err(|_| E::custom("seed must not be negative"))
            .and_then(|v| self.visit_u64(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Seed, E> {
        // `u64::from_str` also accepts a leading `+`; the contract is `^[0-9]{1,20}$`.
        if v.is_empty() || v.len() > 20 || !v.bytes().all(|b| b.is_ascii_digit()) {
            return Err(E::custom("seed string must match ^[0-9]{1,20}$"));
        }
        v.parse::<u64>()
            .map(Seed)
            .map_err(|_| E::custom("seed string does not fit in 64 bits"))
    }
}

impl<'de> Deserialize<'de> for Seed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(SeedVisitor)
    }
}

impl PartialSchema for Seed {
    fn schema() -> RefOr<Schema> {
        OneOfBuilder::new()
            .item(
                ObjectBuilder::new()
                    .schema_type(Type::String)
                    .pattern(Some("^[0-9]{1,20}$"))
                    .description(Some("Decimal digits; must fit in an unsigned 64-bit integer")),
            )
            .item(
                ObjectBuilder::new()
                    .schema_type(Type::Integer)
                    .minimum(Some(0))
                    .maximum(Some(Seed::MAX_SAFE_INTEGER)),
            )
            .description(Some(
                "A seed: a decimal string, or an integer up to 2^53 - 1 (send larger seeds as strings)",
            ))
            .into()
    }
}

impl ToSchema for Seed {}
