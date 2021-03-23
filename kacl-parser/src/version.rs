use crate::{date::Date, IO_VEC_ERR};
use comrak::nodes::{AstNode, NodeHeading, NodeValue};
use itertools::Itertools;
use std::convert::TryFrom;
use versions::SemVer;

#[derive(Debug, Clone)]
pub enum Version {
    Unreleased,
    Released(SemVer, Option<Date>),
}

impl Version {
    pub fn into_released(self) -> Option<(SemVer, Option<Date>)> {
        match self {
            Version::Unreleased => None,
            Version::Released(v, d) => Some((v, d)),
        }
    }
}

#[derive(Debug)]
pub enum VersionParseError {
    /// Block has to be header of 2nd level:
    /// - ## ...
    Header,
    /// Header contents must be a single AST node
    SingleSpan,
    /// Header contents have to match one of following (case-insensitive):
    /// - [\[] "unreleased" [\]]
    /// - [\[] semver::Version [\]] [ "-" chrono::NaiveDate ]
    Format(nom::Err<nom::error::Error<String>>),
    /// For `&[u8] -> &str` conversions
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

impl<'a> TryFrom<&'a AstNode<'a>> for Version {
    type Error = VersionParseError;

    fn try_from(node: &'a AstNode<'a>) -> Result<Self, Self::Error> {
        use nom::{named, opt};

        let data = match node.data.borrow().value {
            NodeValue::Heading(NodeHeading { level: 2, .. }) => node
                .children()
                .exactly_one()
                .map_err(|_| VersionParseError::SingleSpan)?,
            _ => return Err(VersionParseError::Header),
        };
        let data = {
            let mut s = Vec::new();
            comrak::format_html(data, &comrak::ComrakOptions::default(), &mut s).expect(IO_VEC_ERR);
            String::from_utf8(s).map_err(|e| e.utf8_error())?
        };
        // let data = match data {
        //     [comrak::Span::Text(data)] => data.as_str(),
        //     _ => return Err(VersionParseError::SingleSpan),
        // };

        fn parse_unreleased(i: &str) -> nom::IResult<&[u8], ()> {
            use nom::{character::complete::char, tag_no_case};

            named!(unreleased, tag_no_case!("unreleased"));

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

        // TODO: do not use `iso8601`: a) parsers work with u8 b) owner won't expose
        // needed functions as public
        fn parse_date(i: &str) -> nom::IResult<&str, Date> {
            use nom::character::complete::{char, space0};

            let (i, _) = space0(i)?;
            let (i, _) = char('-')(i)?;
            let (i, _) = space0(i)?;
            let (i, date) = Date::parse(i)?;

            Ok((i, date))
        }

        named!(parse_date_opt<&str, Option<Date>>, opt!(parse_date));

        if let Ok((_, ())) = parse_unreleased(&data) {
            return Ok(Version::Unreleased);
        }

        let (data, version) = parse_released(&data)?;
        let (_, opt_date) = parse_date_opt(data)?;

        Ok(Version::Released(version, opt_date))
    }
}
