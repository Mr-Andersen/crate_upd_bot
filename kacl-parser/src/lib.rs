//! KACL stands for for [keepachangelog](https://keepachangelog.com/en/1.0.0/)
use comrak::nodes::{AstNode, NodeHeading, NodeValue};
use itertools::Itertools;
use parsers::Date;
use std::convert::TryFrom;
use versions::SemVer;

mod parsers;

const IO_VEC_ERR: &str = "IO errors shouldn't be possible when writing to Vec";

#[derive(Debug, Clone)]
pub enum Version {
    Unreleased,
    Released(SemVer, Option<Date>),
}

impl Version {
    pub fn into_released(self) -> Option<(SemVer, Option<Date>)> {
        match self {
            Version::Unreleased => None,
            Version::Released(v, d) => Some((v, d))
        }
    }
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

#[derive(Debug, Clone)]
pub struct Changelog<I>(Option<(Version, I)>);

impl<'a, I: Iterator<Item = &'a AstNode<'a>>> Changelog<I> {
    /// Parses `comrak::Block` until `Version` parser succeeds,
    /// ignoring all other problems (e.g. not `# Changelog` as first header)
    pub fn new(mut blocks: I) -> Self {
        loop {
            let block = match blocks.next() {
                Some(block) => block,
                None => return Changelog(None),
            };
            if let Ok(version) = Version::try_from(block) {
                return Changelog(Some((version, blocks)));
            }
        }
    }
}

impl<'a, I: Iterator<Item = &'a AstNode<'a>>> Iterator for Changelog<I> {
    type Item = (Version, Vec<&'a AstNode<'a>>);

    fn next(&mut self) -> Option<Self::Item> {
        let (version, mut blocks) = match self.0.take() {
            Some(v) => v,
            None => return None,
        };

        let mut contents = Vec::new();

        loop {
            let block = match blocks.next() {
                Some(block) => block,
                None => return Some((version, contents)),
            };
            if let Ok(new_version) = Version::try_from(block) {
                self.0 = Some((new_version, blocks));
                return Some((version, contents));
            }
            contents.push(block);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name() {
        let src = include_str!("test.md");
        let arena = comrak::Arena::new();
        Changelog::new(
            comrak::parse_document(&arena, src, &comrak::ComrakOptions::default()).children(),
        )
        .for_each(|(v, nodes)| {
            println!("\n<h2>Version = {:?}</h2>", v);
            let mut s = Vec::new();
            nodes.into_iter().for_each(|node| {
                comrak::format_html(node, &comrak::ComrakOptions::default(), &mut s)
                    .expect(IO_VEC_ERR);
                s.push(b'\n');
            });
            println!("{}", String::from_utf8(s).expect("Nooo"));
        });
    }

    #[test]
    fn top_release() {
        let src = include_str!("test.md");
        let arena = comrak::Arena::new();
        let (v, blocks) = Changelog::new(
            comrak::parse_document(&arena, src, &comrak::ComrakOptions::default()).children(),
        )
            .filter(|(version, _)| matches!(version, Version::Released(..)))
            .next().unwrap();
        let (sv, d) = v.into_released().unwrap();
        print!("{}", sv);
        if let Some(d) = d {
            println!(" - {}", d);
        } else {
            println!("")
        }

        let mut s = Vec::new();

        blocks.into_iter().for_each(|node| {
            comrak::format_html(node, &comrak::ComrakOptions::default(), &mut s)
                .expect(IO_VEC_ERR);
            s.push(b'\n');
        });

        println!("{}", String::from_utf8(s).unwrap());
    }
}
