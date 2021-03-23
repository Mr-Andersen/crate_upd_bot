//! KACL stands for for [keepachangelog](https://keepachangelog.com/en/1.0.0/)
use comrak::nodes::AstNode;
use std::convert::TryFrom;
use version::Version;

mod date;
mod version;

const IO_VEC_ERR: &str = "IO errors shouldn't be possible when writing to Vec";

#[derive(Debug, Clone)]
pub struct Changelog<I>(Option<(Version, I)>);

impl<'a, I: Iterator<Item = &'a AstNode<'a>>> Changelog<I> {
    /// Parses top-level AST node until `Version` parser succeeds,
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
        let (version, mut blocks) = self.0.take()?;

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
        let src = include_str!("../../CHANGELOG.md");
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
    fn top_released() {
        let src = include_str!("../../CHANGELOG.md");
        let arena = comrak::Arena::new();
        let (v, blocks) = Changelog::new(
            comrak::parse_document(&arena, src, &comrak::ComrakOptions::default()).children(),
        )
        .filter(|(version, _)| matches!(version, Version::Released(..)))
        .next()
        .unwrap();
        let (sv, d) = v.into_released().unwrap();
        print!("{}", sv);
        if let Some(d) = d {
            println!(" - {}", d);
        } else {
            println!();
        }

        let mut s = Vec::new();

        blocks.into_iter().for_each(|node| {
            comrak::format_html(node, &comrak::ComrakOptions::default(), &mut s).expect(IO_VEC_ERR);
            s.push(b'\n');
        });

        println!("{}", String::from_utf8(s).unwrap());
    }
}
