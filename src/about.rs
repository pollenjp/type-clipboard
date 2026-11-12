//! 「ヘルプ」→「バージョン情報」に出す文言。名前・版などは Cargo.toml の [package] から取る (build のときに
//! cargo が環境変数で渡す)。

/// 「バージョン情報」の箱の題と本文。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct About {
    pub title: String,
    pub text: String,
}

/// Cargo.toml の [package] のうち、「バージョン情報」に出すもの。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Package<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub description: &'a str,
    pub repository: &'a str,
    pub license: &'a str,
}

/// このアプリの [package] (build したときの Cargo.toml の値)。
pub const PACKAGE: Package<'static> = Package {
    name: env!("CARGO_PKG_NAME"),
    version: env!("CARGO_PKG_VERSION"),
    description: env!("CARGO_PKG_DESCRIPTION"),
    repository: env!("CARGO_PKG_REPOSITORY"),
    license: env!("CARGO_PKG_LICENSE"),
};

impl Package<'_> {
    /// 「バージョン情報」の題と本文。1 行目に名前と版、その下に説明を置き、1 行空けて repo と license を置く。
    /// Cargo.toml に書いていない項目 (空の値) は出さない。
    pub fn about(&self) -> About {
        let head = format!("{} {}", self.name, self.version);
        let license = if self.license.is_empty() {
            String::new()
        } else {
            format!("ライセンス: {}", self.license)
        };
        let paragraphs = [
            [head.as_str(), self.description],
            [self.repository, license.as_str()],
        ];
        let text = paragraphs
            .iter()
            .map(|lines| {
                lines
                    .iter()
                    .copied()
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|paragraph| !paragraph.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        About {
            title: format!("{} のバージョン情報", self.name),
            text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: Package<'static> = Package {
        name: "app",
        version: "1.2.3",
        description: "説明の文",
        repository: "https://example.com/app",
        license: "MIT",
    };

    #[test]
    fn name_and_version_come_first_then_the_links() {
        let about = SAMPLE.about();
        assert_eq!(about.title, "app のバージョン情報");
        assert_eq!(
            about.text,
            "app 1.2.3\n説明の文\n\nhttps://example.com/app\nライセンス: MIT"
        );
    }

    /// Cargo.toml に書いていない項目 (空の値) は、空の行も「ライセンス: 」の札も出さない。
    #[test]
    fn fields_missing_from_cargo_toml_are_left_out() {
        let bare = Package {
            description: "",
            repository: "",
            license: "",
            ..SAMPLE
        };
        assert_eq!(bare.about().text, "app 1.2.3");
        let no_description = Package {
            description: "",
            ..SAMPLE
        };
        assert_eq!(
            no_description.about().text,
            "app 1.2.3\n\nhttps://example.com/app\nライセンス: MIT"
        );
        let no_links = Package {
            repository: "",
            license: "",
            ..SAMPLE
        };
        assert_eq!(no_links.about().text, "app 1.2.3\n説明の文");
    }

    /// 出す版・説明・repo・ライセンスは、Cargo.toml の [package] に書いた値。空の項目は出さないので、取り違えると
    /// 黙って行が消える (例: repository の代わりに、書いていない homepage を読む)。Cargo.toml を読んで比べる。
    #[test]
    fn this_app_shows_what_cargo_toml_says() {
        let manifest = include_str!("../Cargo.toml");
        let field = |key: &str| {
            let prefix = format!("{key} = \"");
            manifest
                .lines()
                .skip_while(|line| line.trim() != "[package]")
                .skip(1)
                .take_while(|line| !line.starts_with('['))
                .find_map(|line| line.strip_prefix(prefix.as_str()))
                .and_then(|rest| rest.strip_suffix('"'))
                .unwrap_or_else(|| panic!("Cargo.toml の [package] に {key} がある"))
                .to_string()
        };
        let text = PACKAGE.about().text;
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines.first().copied(),
            Some(format!("type-clipboard {}", field("version")).as_str())
        );
        for expected in [
            field("description"),
            field("repository"),
            format!("ライセンス: {}", field("license")),
        ] {
            assert!(lines.contains(&expected.as_str()), "{expected:?} が無い");
        }
    }
}
