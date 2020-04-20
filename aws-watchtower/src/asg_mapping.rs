use serde_derive::{Deserialize, Serialize};

#[derive(PartialEq, Deserialize, Serialize, Debug)]
pub struct Mappings {
    #[serde(rename = "mapping")]
    pub items: Vec<Mapping>,
}

impl Mappings {
    pub fn map(&self, text: &str) -> Option<&Mapping> {
        for m in &self.items {
            if m.matches(text) {
                return Some(&m);
            }
        }

        None
    }
}

#[derive(PartialEq, Deserialize, Serialize, Debug)]
pub struct Mapping {
    pub search:      String,
    pub tag_name:    String,
    pub host_prefix: String,
}

impl Mapping {
    pub fn matches(&self, text: &str) -> bool { text.find(&self.search).is_some() }
}

#[cfg(test)]
mod test {
    use super::*;

    use spectral::prelude::*;
    use toml;

    #[test]
    fn test_load_mappings() {
        let toml = r#"[[mapping]]
search = "webserver"
tag_name = "webserver"
host_prefix = "webserver-"

[[mapping]]
search = "import"
tag_name = "import"
host_prefix = "import-"
"#;
        let items = vec![
            Mapping {
                search:      "webserver".to_string(),
                tag_name:    "webserver".to_string(),
                host_prefix: "webserver-".to_string(),
            },
            Mapping {
                search:      "import".to_string(),
                tag_name:    "import".to_string(),
                host_prefix: "import-".to_string(),
            },
        ];
        let expected = Mappings { items };

        let mappings: Result<Mappings, _> = toml::from_str(&toml);

        asserting("mappings loads successfully")
            .that(&mappings)
            .is_ok()
            .is_equal_to(&expected);
    }

    #[test]
    fn matches_true() {
        let text = "project-staging-asg-webserver-20181205092547277600000001";

        let m = Mapping {
            search:      "webserver".to_string(),
            tag_name:    "webserver".to_string(),
            host_prefix: "webserver-".to_string(),
        };
        let res = m.matches(text);

        asserting("mapping matches").that(&res).is_true();
    }

    #[test]
    fn matches_false() {
        let text = "project-staging-asg-import_server-b40-20181125202055415500000001";

        let m = Mapping {
            search:      "webserver".to_string(),
            tag_name:    "webserver".to_string(),
            host_prefix: "webserver-".to_string(),
        };
        let res = m.matches(text);

        asserting("mapping does not match").that(&res).is_false();
    }

    #[test]
    fn map() {
        let items = vec![
            Mapping {
                search:      "webserver".to_string(),
                tag_name:    "webserver".to_string(),
                host_prefix: "webserver-".to_string(),
            },
            Mapping {
                search:      "import".to_string(),
                tag_name:    "import".to_string(),
                host_prefix: "import-".to_string(),
            },
        ];
        let mappings = Mappings { items };
        let expected = Mapping {
            search:      "webserver".to_string(),
            tag_name:    "webserver".to_string(),
            host_prefix: "webserver-".to_string(),
        };

        let text = "project-staging-asg-webserver-20181205092547277600000001";
        let res = mappings.map(text);
        asserting("mapping successfully maps")
            .that(&res)
            .is_some()
            .is_equal_to(&expected);

        let text = "project-staging-asg-app_server-b40-20181125202055415500000001";
        let res = mappings.map(text);
        asserting("mapping successfully maps").that(&res).is_none();
    }
}
