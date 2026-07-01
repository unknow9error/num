#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decimal {
    coefficient: i128,
    scale: u32,
}

impl Decimal {
    pub fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if value.is_empty() {
            return Err("expected a decimal value".to_string());
        }
        let negative = value.starts_with('-');
        let unsigned = value.strip_prefix('-').unwrap_or(value);
        if unsigned.is_empty() || unsigned == "." {
            return Err("expected digits before or after the decimal point".to_string());
        }
        let mut parts = unsigned.split('.');
        let whole = parts.next().unwrap_or_default();
        let fraction = parts.next();
        if parts.next().is_some() {
            return Err("expected at most one decimal point".to_string());
        }
        let fraction = fraction.unwrap_or_default();
        if whole.is_empty() && fraction.is_empty() {
            return Err("expected digits before or after the decimal point".to_string());
        }
        if !whole.chars().all(|ch| ch.is_ascii_digit())
            || !fraction.chars().all(|ch| ch.is_ascii_digit())
        {
            return Err("expected ASCII decimal digits".to_string());
        }

        let digits = format!("{}{}", if whole.is_empty() { "0" } else { whole }, fraction);
        let mut coefficient = digits
            .parse::<i128>()
            .map_err(|_| "decimal coefficient is out of range".to_string())?;
        if negative {
            coefficient = -coefficient;
        }
        Ok(Self::new(coefficient, fraction.len() as u32))
    }

    pub fn add(&self, other: &Self) -> Result<Self, String> {
        let (left, right, scale) = self.align(other)?;
        Ok(Self::new(
            left.checked_add(right)
                .ok_or_else(|| "decimal addition overflowed".to_string())?,
            scale,
        ))
    }

    pub fn subtract(&self, other: &Self) -> Result<Self, String> {
        let (left, right, scale) = self.align(other)?;
        Ok(Self::new(
            left.checked_sub(right)
                .ok_or_else(|| "decimal subtraction overflowed".to_string())?,
            scale,
        ))
    }

    pub fn multiply(&self, other: &Self) -> Result<Self, String> {
        Ok(Self::new(
            self.coefficient
                .checked_mul(other.coefficient)
                .ok_or_else(|| "decimal multiplication overflowed".to_string())?,
            self.scale
                .checked_add(other.scale)
                .ok_or_else(|| "decimal scale overflowed".to_string())?,
        ))
    }

    pub fn divide(&self, other: &Self) -> Result<Self, String> {
        if other.coefficient == 0 {
            return Err("decimal division by zero".to_string());
        }
        let precision = 9u32;
        let numerator = self
            .coefficient
            .checked_mul(pow10(precision)?)
            .ok_or_else(|| "decimal division overflowed".to_string())?;
        let coefficient = numerator / other.coefficient;
        let scale = self
            .scale
            .checked_add(precision)
            .and_then(|scale| scale.checked_sub(other.scale))
            .ok_or_else(|| "decimal scale overflowed".to_string())?;
        Ok(Self::new(coefficient, scale))
    }

    pub fn multiply_i128_round(&self, value: i128) -> Result<i128, String> {
        let scaled = self
            .coefficient
            .checked_mul(value)
            .ok_or_else(|| "decimal multiplication overflowed".to_string())?;
        let divisor = pow10(self.scale)?;
        let quotient = scaled / divisor;
        let remainder = scaled % divisor;
        let abs_remainder = remainder.abs();
        let round_up = abs_remainder
            .checked_mul(2)
            .ok_or_else(|| "decimal rounding overflowed".to_string())?
            >= divisor;
        if !round_up {
            return Ok(quotient);
        }
        if scaled >= 0 {
            quotient
                .checked_add(1)
                .ok_or_else(|| "decimal rounded result overflowed".to_string())
        } else {
            quotient
                .checked_sub(1)
                .ok_or_else(|| "decimal rounded result overflowed".to_string())
        }
    }

    pub fn cmp(&self, other: &Self) -> Result<std::cmp::Ordering, String> {
        let (left, right, _) = self.align(other)?;
        Ok(left.cmp(&right))
    }

    fn new(coefficient: i128, scale: u32) -> Self {
        let mut value = Self { coefficient, scale };
        value.normalize();
        value
    }

    fn normalize(&mut self) {
        while self.scale > 0 && self.coefficient % 10 == 0 {
            self.coefficient /= 10;
            self.scale -= 1;
        }
    }

    fn align(&self, other: &Self) -> Result<(i128, i128, u32), String> {
        let scale = self.scale.max(other.scale);
        let left = self
            .coefficient
            .checked_mul(pow10(scale - self.scale)?)
            .ok_or_else(|| "decimal scale alignment overflowed".to_string())?;
        let right = other
            .coefficient
            .checked_mul(pow10(scale - other.scale)?)
            .ok_or_else(|| "decimal scale alignment overflowed".to_string())?;
        Ok((left, right, scale))
    }
}

impl std::fmt::Display for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.scale == 0 {
            return write!(f, "{}", self.coefficient);
        }
        let negative = self.coefficient < 0;
        let digits = self.coefficient.abs().to_string();
        let scale = self.scale as usize;
        if negative {
            write!(f, "-")?;
        }
        if digits.len() <= scale {
            write!(f, "0.")?;
            for _ in 0..(scale - digits.len()) {
                write!(f, "0")?;
            }
            write!(f, "{digits}")
        } else {
            let split = digits.len() - scale;
            write!(f, "{}.{}", &digits[..split], &digits[split..])
        }
    }
}

fn pow10(exp: u32) -> Result<i128, String> {
    let mut value = 1i128;
    for _ in 0..exp {
        value = value
            .checked_mul(10)
            .ok_or_else(|| "decimal scale is out of range".to_string())?;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::Decimal;

    #[test]
    fn parses_and_prints_without_float_rounding() {
        assert_eq!(Decimal::parse("1.2300").unwrap().to_string(), "1.23");
        assert_eq!(Decimal::parse("-0.050").unwrap().to_string(), "-0.05");
    }

    #[test]
    fn computes_decimal_arithmetic() {
        let left = Decimal::parse("10.50").unwrap();
        let right = Decimal::parse("2.25").unwrap();

        assert_eq!(left.add(&right).unwrap().to_string(), "12.75");
        assert_eq!(left.subtract(&right).unwrap().to_string(), "8.25");
        assert_eq!(left.multiply(&right).unwrap().to_string(), "23.625");
        assert_eq!(
            left.divide(&Decimal::parse("2").unwrap())
                .unwrap()
                .to_string(),
            "5.25"
        );
    }

    #[test]
    fn multiplies_integer_with_decimal_rounding() {
        assert_eq!(
            Decimal::parse("450.25")
                .unwrap()
                .multiply_i128_round(10000)
                .unwrap(),
            4502500
        );
        assert_eq!(
            Decimal::parse("1.005")
                .unwrap()
                .multiply_i128_round(100)
                .unwrap(),
            101
        );
    }
}
