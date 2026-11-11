use std::cmp::Ordering;
use std::fmt;

use num_bigint::BigInt;
use num_traits::{FromPrimitive, ToPrimitive};

#[derive(Clone, Debug)]
pub enum Number {
    Whole(BigInt),
    Decimal(f64),
}

impl Number {
    pub fn parse(text: &str) -> Option<Self> {
        if text.contains('.') {
            let value = text.parse::<f64>().ok()?;
            value.is_finite().then_some(Self::Decimal(value))
        } else {
            text.parse().ok().map(Self::Whole)
        }
    }

    pub fn adjust(self, amount: Self, subtract: bool) -> Option<Self> {
        match (self, amount) {
            (Self::Whole(left), Self::Whole(right)) => {
                let result = if subtract { left - right } else { left + right };
                Some(Self::Whole(result))
            }
            (left, right) => {
                let left = left.as_decimal()?;
                let right = right.as_decimal()?;
                let result = if subtract { left - right } else { left + right };
                result.is_finite().then_some(Self::Decimal(result))
            }
        }
    }

    fn as_decimal(&self) -> Option<f64> {
        match self {
            Self::Whole(value) => value.to_f64().filter(|value| value.is_finite()),
            Self::Decimal(value) => Some(*value),
        }
    }

    pub fn compare(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Whole(left), Self::Whole(right)) => left.cmp(right),
            (Self::Decimal(left), Self::Decimal(right)) => {
                left.partial_cmp(right).expect("Code decimals are finite")
            }
            (Self::Whole(left), Self::Decimal(right)) => compare_whole_decimal(left, *right),
            (Self::Decimal(left), Self::Whole(right)) => {
                compare_whole_decimal(right, *left).reverse()
            }
        }
    }
}

fn compare_whole_decimal(whole: &BigInt, decimal: f64) -> Ordering {
    let integral = BigInt::from_f64(decimal).expect("Code decimals are finite");
    // Compare the whole parts exactly, then account for truncation toward zero.
    whole.cmp(&integral).then_with(|| {
        0.0_f64
            .partial_cmp(&decimal.fract())
            .expect("Code decimals are finite")
    })
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Whole(value) => write!(f, "{value}"),
            Self::Decimal(value) => {
                let text = format!("{value:?}");
                if let Some((mantissa, exponent)) = text.split_once('e') {
                    let exponent: i32 = exponent.parse().expect("valid decimal exponent");
                    write!(f, "{mantissa}e{exponent:+03}")
                } else {
                    f.write_str(&text)
                }
            }
        }
    }
}
