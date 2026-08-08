use std::{collections::BTreeSet, env, io};

use crate::ConfigError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ParserName {
    PumpFun,
}

impl ParserName {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PumpFun => "pumpfun",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParserConfig {
    parsers: Vec<ParserName>,
}

impl ParserConfig {
    pub fn new(parsers: Vec<ParserName>) -> Result<Self, ConfigError> {
        if parsers.is_empty() {
            return Err(ConfigError::EmptyParsers);
        }

        let mut unique = BTreeSet::new();
        for parser in &parsers {
            if !unique.insert(*parser) {
                return Err(ConfigError::DuplicateParser {
                    name: parser.as_str().to_owned(),
                });
            }
        }

        Ok(Self { parsers })
    }

    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        if value.trim().is_empty() {
            return Err(ConfigError::EmptyParsers);
        }

        let mut parsers = Vec::new();
        for (index, raw_name) in value.split(',').enumerate() {
            let name = raw_name.trim();
            if name.is_empty() {
                return Err(ConfigError::EmptyParserName { index });
            }

            let parser = match name {
                "pumpfun" => ParserName::PumpFun,
                _ => {
                    return Err(ConfigError::UnknownParser {
                        name: name.to_owned(),
                    });
                }
            };
            parsers.push(parser);
        }

        Self::new(parsers)
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        match dotenvy::dotenv() {
            Ok(_) => {}
            Err(dotenvy::Error::Io(error)) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(ConfigError::Dotenv(error.to_string())),
        }

        match env::var("PARSERS") {
            Ok(value) => Self::parse(&value),
            Err(env::VarError::NotPresent) => Err(ConfigError::MissingParsers),
            Err(env::VarError::NotUnicode(_)) => Err(ConfigError::NonUnicodeParsers),
        }
    }

    pub fn parsers(&self) -> &[ParserName] {
        &self.parsers
    }
}

#[cfg(test)]
mod tests {
    use std::{env, ffi::OsString, fs, path::PathBuf, sync::Mutex};

    use super::*;
    use crate::BlockParser;

    static ENVIRONMENT: Mutex<()> = Mutex::new(());

    struct EnvironmentGuard {
        value: Option<OsString>,
        directory: PathBuf,
        temporary_directory: Option<PathBuf>,
    }

    impl EnvironmentGuard {
        fn capture() -> Self {
            Self {
                value: env::var_os("PARSERS"),
                directory: env::current_dir().unwrap(),
                temporary_directory: None,
            }
        }

        fn use_temporary_directory(&mut self) {
            let directory =
                env::temp_dir().join(format!("the-cow-parser-dotenv-{}", std::process::id()));
            fs::create_dir_all(&directory).unwrap();
            env::set_current_dir(&directory).unwrap();
            self.temporary_directory = Some(directory);
        }

        fn write_dotenv(&self) {
            fs::write(
                self.temporary_directory.as_ref().unwrap().join(".env"),
                "PARSERS=pumpfun\n",
            )
            .unwrap();
        }
    }

    impl Drop for EnvironmentGuard {
        fn drop(&mut self) {
            env::set_current_dir(&self.directory).unwrap();
            match self.value.take() {
                Some(value) => env::set_var("PARSERS", value),
                None => env::remove_var("PARSERS"),
            }
            if let Some(directory) = self.temporary_directory.take() {
                fs::remove_dir_all(directory).unwrap();
            }
        }
    }

    #[test]
    fn parses_supported_names_and_rejects_invalid_configuration() {
        assert_eq!(
            ParserConfig::parse(" pumpfun ").unwrap().parsers(),
            &[ParserName::PumpFun]
        );
        assert_eq!(ParserConfig::parse("  "), Err(ConfigError::EmptyParsers));
        assert_eq!(
            ParserConfig::parse("pumpfun,"),
            Err(ConfigError::EmptyParserName { index: 1 })
        );
        assert_eq!(
            ParserConfig::parse("pumpswap"),
            Err(ConfigError::UnknownParser {
                name: "pumpswap".to_owned()
            })
        );
        assert_eq!(
            ParserConfig::parse("pumpfun,pumpfun"),
            Err(ConfigError::DuplicateParser {
                name: "pumpfun".to_owned()
            })
        );
        assert_eq!(ParserConfig::new(vec![]), Err(ConfigError::EmptyParsers));
    }

    #[test]
    fn loads_block_parser_configuration_from_the_environment() {
        let _lock = ENVIRONMENT.lock().unwrap();
        let mut guard = EnvironmentGuard::capture();
        guard.use_temporary_directory();

        env::remove_var("PARSERS");
        assert!(matches!(
            BlockParser::from_env(),
            Err(crate::ParserInitError::Config(ConfigError::MissingParsers))
        ));

        env::set_var("PARSERS", "pumpfun");
        assert!(BlockParser::from_env().is_ok());

        env::remove_var("PARSERS");
        guard.write_dotenv();
        assert!(BlockParser::from_env().is_ok());
    }
}
