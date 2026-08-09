//! Minimal X25519 public-key derivation used by the AirPlay `/auth-setup` probe.
//!
//! This is intentionally private to the RAOP handshake and is not a general-purpose
//! cryptography API. The arithmetic follows the Montgomery ladder from RFC 7748.

type FieldElement = [i64; 16];

const CURVE_121665: FieldElement = [0xdb41, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

pub(crate) fn x25519_public_key(secret: &[u8; 32]) -> [u8; 32] {
    let mut base_point = [0u8; 32];
    base_point[0] = 9;
    scalar_mult(secret, &base_point)
}

fn scalar_mult(scalar: &[u8; 32], point: &[u8; 32]) -> [u8; 32] {
    let mut clamped = *scalar;
    clamped[0] &= 248;
    clamped[31] = (clamped[31] & 127) | 64;

    let x = unpack(point);
    let mut a = [0i64; 16];
    let mut b = x;
    let mut c = [0i64; 16];
    let mut d = [0i64; 16];
    a[0] = 1;
    d[0] = 1;

    for bit_index in (0..=254).rev() {
        let swap = i64::from((clamped[bit_index >> 3] >> (bit_index & 7)) & 1);
        conditional_swap(&mut a, &mut b, swap);
        conditional_swap(&mut c, &mut d, swap);

        let e = add(&a, &c);
        a = subtract(&a, &c);
        c = add(&b, &d);
        b = subtract(&b, &d);
        d = square(&e);
        let f = square(&a);
        a = multiply(&c, &a);
        c = multiply(&b, &e);
        let e = add(&a, &c);
        a = subtract(&a, &c);
        b = square(&a);
        c = subtract(&d, &f);
        a = multiply(&c, &CURVE_121665);
        a = add(&a, &d);
        c = multiply(&c, &a);
        a = multiply(&d, &f);
        d = multiply(&b, &x);
        b = square(&e);

        conditional_swap(&mut a, &mut b, swap);
        conditional_swap(&mut c, &mut d, swap);
    }

    c = invert(&c);
    a = multiply(&a, &c);
    pack(&a)
}

fn add(left: &FieldElement, right: &FieldElement) -> FieldElement {
    let mut result = [0i64; 16];
    for index in 0..16 {
        result[index] = left[index] + right[index];
    }
    result
}

fn subtract(left: &FieldElement, right: &FieldElement) -> FieldElement {
    let mut result = [0i64; 16];
    for index in 0..16 {
        result[index] = left[index] - right[index];
    }
    result
}

fn multiply(left: &FieldElement, right: &FieldElement) -> FieldElement {
    let mut product = [0i64; 31];
    for left_index in 0..16 {
        for right_index in 0..16 {
            product[left_index + right_index] += left[left_index] * right[right_index];
        }
    }
    for index in 0..15 {
        product[index] += 38 * product[index + 16];
    }

    let mut result = [0i64; 16];
    result.copy_from_slice(&product[..16]);
    carry(&mut result);
    carry(&mut result);
    result
}

fn square(value: &FieldElement) -> FieldElement {
    multiply(value, value)
}

fn invert(value: &FieldElement) -> FieldElement {
    let mut result = *value;
    for exponent_bit in (0..=253).rev() {
        result = square(&result);
        if exponent_bit != 2 && exponent_bit != 4 {
            result = multiply(&result, value);
        }
    }
    result
}

fn carry(value: &mut FieldElement) {
    for index in 0..16 {
        value[index] += 1 << 16;
        let overflow = value[index] >> 16;
        if index < 15 {
            value[index + 1] += overflow - 1;
        } else {
            value[0] += 38 * (overflow - 1);
        }
        value[index] -= overflow << 16;
    }
}

fn conditional_swap(left: &mut FieldElement, right: &mut FieldElement, swap: i64) {
    let mask = !(swap - 1);
    for index in 0..16 {
        let difference = mask & (left[index] ^ right[index]);
        left[index] ^= difference;
        right[index] ^= difference;
    }
}

fn unpack(bytes: &[u8; 32]) -> FieldElement {
    let mut result = [0i64; 16];
    for index in 0..16 {
        result[index] = i64::from(bytes[2 * index]) + (i64::from(bytes[2 * index + 1]) << 8);
    }
    result[15] &= 0x7fff;
    result
}

fn pack(value: &FieldElement) -> [u8; 32] {
    let mut reduced = *value;
    carry(&mut reduced);
    carry(&mut reduced);
    carry(&mut reduced);

    for _ in 0..2 {
        let mut candidate = [0i64; 16];
        candidate[0] = reduced[0] - 0xffed;
        for index in 1..15 {
            candidate[index] = reduced[index] - 0xffff - ((candidate[index - 1] >> 16) & 1);
            candidate[index - 1] &= 0xffff;
        }
        candidate[15] = reduced[15] - 0x7fff - ((candidate[14] >> 16) & 1);
        let borrow = (candidate[15] >> 16) & 1;
        candidate[14] &= 0xffff;
        conditional_swap(&mut reduced, &mut candidate, 1 - borrow);
    }

    let mut bytes = [0u8; 32];
    for index in 0..16 {
        bytes[2 * index] = (reduced[index] & 0xff) as u8;
        bytes[2 * index + 1] = (reduced[index] >> 8) as u8;
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::x25519_public_key;

    #[test]
    fn matches_rfc_7748_alice_public_key() {
        let secret = decode_hex("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
        let expected =
            decode_hex("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a");
        assert_eq!(x25519_public_key(&secret), expected);
    }

    fn decode_hex(input: &str) -> [u8; 32] {
        assert_eq!(input.len(), 64);
        let mut bytes = [0u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&input[index * 2..index * 2 + 2], 16).unwrap();
        }
        bytes
    }
}
