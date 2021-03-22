//! KACL stands for for [keepachangelog](https://keepachangelog.com/en/1.0.0/)
use std::convert::TryFrom;

// use either::Either;
use iso8601::Date;
use markdown as md;
// use comrak as md;
// use comrak::nodes::{NodeValue, NodeHeading};
use versions::SemVer;

#[derive(Debug, Clone)]
pub enum Version {
    Unreleased,
    Released(SemVer, Option<Date>),
}

#[derive(Debug)]
pub enum VersionParseError {
    /// Block has to be header:
    /// - ## ...
    Header,
    /// Header contents must be a single `markdown::Span`
    SingleSpan,
    /// Header contents have to match one of following (case-insensitive):
    /// - [\[] "unreleased" [\]]
    /// - [\[] semver::Version [\]] [ "-" chrono::NaiveDate ]
    Format(nom::Err<nom::error::Error<String>>),
    /// Cannot prove that underlying Date parser doesn't break utf8
    Utf8(std::str::Utf8Error),
}

impl<S> From<nom::Err<nom::error::Error<S>>> for VersionParseError
where
    S: Into<String>,
{
    fn from(err: nom::Err<nom::error::Error<S>>) -> Self {
        use nom::{error::Error, Err::*};

        VersionParseError::Format(match err {
            Incomplete(needed) => Incomplete(needed),
            Error(Error { input, code }) => Error(Error {
                input: input.into(),
                code,
            }),
            Failure(Error { input, code }) => Failure(Error {
                input: input.into(),
                code,
            }),
        })
    }
}

impl From<std::str::Utf8Error> for VersionParseError {
    fn from(e: std::str::Utf8Error) -> Self {
        VersionParseError::Utf8(e)
    }
}

fn between<I, O, V, LO, L, RO, R>(left: L, value: V, right: R, i: I) -> nom::IResult<I, O>
where
    L: FnOnce(I) -> nom::IResult<I, LO>,
    V: FnOnce(I) -> nom::IResult<I, O>,
    R: FnOnce(I) -> nom::IResult<I, RO>,
{
    let (i, _) = left(i)?;
    let (i, v) = value(i)?;
    let (i, _) = right(i)?;
    Ok((i, v))
}

impl TryFrom<&md::Block> for Version {
    type Error = VersionParseError;

    fn try_from(blk: &md::Block) -> Result<Self, Self::Error> {
        use nom::{named, opt};

        let data = match blk {
            md::Block::Header(data, 2) => data.as_slice(),
            _ => return Err(VersionParseError::Header),
        };
        let data = match data {
            [md::Span::Text(data)] => data.as_str(),
            _ => return Err(VersionParseError::SingleSpan),
        };

        fn parse_unreleased(i: &str) -> nom::IResult<&[u8], ()> {
            use nom::{character::complete::char, tag_no_case};

            named!(unreleased, tag_no_case!(&"unreleased"[..]));

            let (i, _) = unreleased(i.as_ref())
                .or_else(|_| between(char('['), unreleased, char(']'), i.as_ref()))?;

            Ok((i, ()))
        }

        fn parse_released(i: &str) -> nom::IResult<&str, SemVer> {
            let (i, version) = SemVer::parse(i).or_else(|_| {
                between(
                    nom::character::complete::char('['),
                    SemVer::parse,
                    nom::character::complete::char(']'),
                    i,
                )
            })?;

            Ok((i, version))
        }

        fn parse_date(i: &str) -> nom::IResult<&str, Date> {
            use nom::{
                character::complete::{char, space0},
                error::Error
            };

            let (i, _) = space0(i)?;
            let (i, _) = char('-')(i)?;
            let (i, _) = space0(i)?;
            let (i, date) = iso8601::parsers::parse_date(i.as_ref()).map_err(|e| {
                e.map(|Error { input, code }| Error {
                    input: std::str::from_utf8(input).unwrap(),
                    code,
                })
            })?;

            Ok((std::str::from_utf8(i).unwrap(), date))
        }

        named!(parse_date_opt<&str, Option<Date>>, opt!(parse_date));

        if let Ok((_, ())) = parse_unreleased(data) {
            return Ok(Version::Unreleased);
        }

        let (data, version) = parse_released(data)?;
        let (_, opt_date) = parse_date_opt(data.as_ref())?;

        Ok(Version::Released(version, opt_date))
    }
}

#[derive(Debug, Clone)]
// Store (next_version, rest) maybe? (less unwraps)
// Changelog(Option<(Version, I)>)
pub struct Changelog<I>(I);

impl<I: Iterator<Item = md::Block>> Changelog<std::iter::Peekable<I>> {
    /// Parses `md::Block` until `Version` parser succeeds,
    /// ignoring all other problems (e.g. not `# Changelog` as first header)
    pub fn new(blks: I) -> Option<Self> {
        let mut blks = blks.peekable();

        loop {
            let b = blks.peek()?;
            if Version::try_from(b).is_ok() {
                return Some(Changelog(blks));
            }
            blks.next().unwrap();
        }
    }
}

impl<I: Iterator<Item = md::Block>> Iterator for Changelog<std::iter::Peekable<I>> {
    type Item = (Version, Vec<md::Block>);

    fn next(&mut self) -> Option<Self::Item> {
        let version = Version::try_from(&self.0.next()?).expect("Next entry to be valid Version");
        let mut contents = Vec::new();

        loop {
            let b = match self.0.peek() {
                Some(b) => b,
                None => return Some((version, contents)),
            };
            if Version::try_from(b).is_ok() {
                return Some((version, contents));
            }
            // Correct, because `peek()` returned `Some`
            contents.push(self.0.next().unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name() {
        let src = include_str!("test.md");
        Changelog::new(md::tokenize(src).into_iter())
            .unwrap()
            .for_each(|(v, txt)| {
                println!("\n<h1>Version = {:?}</h1>", v);
                println!("{}", md::to_html(&md::generate_markdown(txt)));
            });
    }
}
