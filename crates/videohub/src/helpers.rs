// Nom helpers, mainly.
use nom::{
    branch::alt,
    bytes::streaming::{tag, take_while1},
    character::complete as char_comp,
    combinator::map_res,
    Err, IResult, Needed, Parser,
};

/// Match either LF or CRLF.
// (Streaming)
pub fn any_newline(i: &[u8]) -> IResult<&[u8], &[u8]> {
    alt((tag(&b"\r\n"[..]), tag(&b"\n"[..]))).parse(i)
}

/// Take until first newline character
// (Streaming)
pub fn take_until_newline(i: &[u8]) -> IResult<&[u8], &[u8]> {
    take_while1(|c| c != b'\r' && c != b'\n').parse(i)
}

/// Take everything until a double newline / empty line.
/// Consumes the empty line as well.
/// (Streaming)
pub fn take_until_empty_line(i: &[u8]) -> IResult<&[u8], &[u8]> {
    let len = i.len();
    for pos in 0..len {
        if pos + 1 < len && &i[pos..=pos + 1] == b"\n\n" {
            let (head, rest) = i.split_at(pos + 1);
            // drop the second "\n"
            return Ok((&rest[1..], head));
        }
        if pos + 3 < len && &i[pos..=pos + 3] == b"\r\n\r\n" {
            let (head, rest) = i.split_at(pos + 2);
            // drop the second "\r\n"
            return Ok((&rest[2..], head));
        }
    }
    Err(Err::Incomplete(Needed::Unknown))
}

/// Parse ASCII digits to u32
/// (Complete)
pub fn parse_u32(i: &[u8]) -> IResult<&[u8], u32> {
    map_res(char_comp::digit1, |d: &[u8]| {
        // Due to digit1 allowing only [0-9]+, the unwrap will never error.
        str::from_utf8(d).unwrap().parse()
    })(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_u32() {
        let (mut rem, mut num) = parse_u32(b"123\n").unwrap();
        assert_eq!(num, 123);
        assert_eq!(rem, b"\n");

        (rem, num) = parse_u32(b"16").unwrap();
        assert_eq!(num, 16);
        assert_eq!(rem, b"");
    }

    #[test]
    fn test_take_until_empty_line() {
        let input = b"foo\nbar\n\nbaz\n\n";
        let (mut rem, mut head) = take_until_empty_line(input).unwrap();
        assert_eq!(head, b"foo\nbar\n");
        assert_eq!(rem, b"baz\n\n");

        let input = b"hello\r\n\r\nworld";
        (rem, head) = take_until_empty_line(input).unwrap();
        assert_eq!(head, b"hello\r\n");
        assert_eq!(rem, b"world");

        let input = b"hello\r\n\r\nworld";
        (rem, head) = take_until_empty_line(input).unwrap();
        assert_eq!(head, b"hello\r\n");
        assert_eq!(rem, b"world");

        let input = b"no blank line";
        assert!(take_until_empty_line(input).is_err());
    }
}
