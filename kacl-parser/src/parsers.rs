use nom::{
    bytes::complete::{tag, take},
    combinator::map,
    error::{Error, ErrorKind},
    sequence::tuple,
    Err, IResult,
};
use std::{
    fmt,
    ops::{Add, Mul, Sub},
    str,
};

#[derive(Clone, Copy, Debug)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}-{}", self.year, self.month, self.day)
    }
}

fn decimal_from_bytes<'s, I>(src: &'s [u8], bts: &[u8]) -> Result<I, Err<Error<&'s [u8]>>>
where
    I: From<u8> + Add<I, Output = I> + Mul<I, Output = I> + Sub<I, Output = I>,
{
    bts.iter().try_fold(0u8.into(), |acc, &digit| match digit {
        b'0'..=b'9' => Ok(acc * 10u8.into() + digit.into() - b'0'.into()),
        _ => Err(Err::Error(Error::new(src, ErrorKind::Digit))),
    })
}

fn decimal_n<I>(n: usize, i: &[u8]) -> IResult<&[u8], I>
where
    I: From<u8> + Add<I, Output = I> + Mul<I, Output = I> + Sub<I, Output = I>,
{
    let (i, digits) = take(n)(i)?;
    Ok((i, decimal_from_bytes(i, digits)?))
}

impl Date {
    pub fn parse(i: &str) -> IResult<&str, Date> {
        map(
            tuple((
                |i| decimal_n(4, i),
                tag(b"-"),
                |i| decimal_n(2, i),
                tag(b"-"),
                |i| decimal_n(2, i),
            )),
            |(year, _, month, _, day)| Date { year, month, day },
        )(i.as_bytes())
        .map(|(i, d)| (str::from_utf8(i).unwrap(), d))
        .map_err(|e| {
            e.map(|Error { input, code }| Error {
                input: str::from_utf8(input).unwrap(),
                code,
            })
        })
    }
}
